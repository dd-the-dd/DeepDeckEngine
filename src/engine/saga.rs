use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{
    CardDefinition, CardInstance, GameEngine, integer_value, triggered_rule_supported,
    type_line_contains, value_kind,
};

const SAGA_ENTRY_PENDING_FLAG: &str = "sagaEntryLorePending";
const READ_AHEAD_ENTRY_PREFIX: &str = "sagaReadAheadEntryChapter:";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SagaChapterPolicy {
    CrossedThresholds,
    ReadAheadSelectedOnly(i32),
}

pub(super) fn saga_final_chapter(source: &CardDefinition) -> Option<i32> {
    type_line_contains(&source.type_line, "Saga").then_some(())?;
    source
        .rules
        .iter()
        .filter(|candidate| value_kind(&candidate["event"]) == Some("sagaChapterReached"))
        .flat_map(|candidate| {
            candidate["event"]["chapters"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(integer_value)
        .max()
}

pub(super) fn saga_chapter_rule_is_final(source: &CardDefinition, rule: &Value) -> bool {
    if value_kind(&rule["event"]) != Some("sagaChapterReached") {
        return false;
    }
    saga_final_chapter(source).is_some_and(|final_chapter| {
        rule["event"]["chapters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(integer_value)
            .any(|chapter| chapter == final_chapter)
    })
}

pub(super) fn saga_chapters_for_transition(
    old_count: i32,
    new_count: i32,
    policy: SagaChapterPolicy,
) -> Vec<i32> {
    if new_count <= old_count {
        return Vec::new();
    }
    match policy {
        SagaChapterPolicy::CrossedThresholds => ((old_count + 1)..=new_count).collect(),
        SagaChapterPolicy::ReadAheadSelectedOnly(chapter)
            if chapter > old_count && chapter <= new_count =>
        {
            vec![chapter]
        }
        SagaChapterPolicy::ReadAheadSelectedOnly(_) => Vec::new(),
    }
}

fn narrowed_saga_chapter_rule(rule: &Value, chapter: i32) -> Value {
    let mut narrowed = rule.clone();
    narrowed["event"]["chapters"] = json!([{
        "kind": "integer",
        "value": chapter,
    }]);
    narrowed
}

impl GameEngine {
    pub(super) fn mark_saga_entry_lore_pending(
        &self,
        card: &mut CardInstance,
        read_ahead_chapter: Option<i32>,
    ) {
        card.flags.insert(SAGA_ENTRY_PENDING_FLAG.to_string(), true);
        if let Some(chapter) = read_ahead_chapter {
            card.flags
                .insert(format!("{READ_AHEAD_ENTRY_PREFIX}{chapter}"), true);
        }
    }

    pub(super) fn initialize_saga_lore_on_entry(&mut self, source: &CardInstance) {
        if !type_line_contains(&source.definition.type_line, "Saga") {
            return;
        }
        let Some((player_index, card_index)) = self.permanent_position(&source.instance_id) else {
            return;
        };
        if self.state.players[player_index].battlefield[card_index]
            .counters
            .contains_key("lore")
        {
            return;
        }
        let flags = self.state.players[player_index].battlefield[card_index]
            .flags
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let selected_chapter = flags.iter().find_map(|flag| {
            flag.strip_prefix(READ_AHEAD_ENTRY_PREFIX)
                .and_then(|chapter| chapter.parse::<i32>().ok())
        });
        self.state.players[player_index].battlefield[card_index]
            .flags
            .remove(SAGA_ENTRY_PENDING_FLAG);
        for flag in flags {
            if flag.starts_with(READ_AHEAD_ENTRY_PREFIX) {
                self.state.players[player_index].battlefield[card_index]
                    .flags
                    .remove(&flag);
            }
        }
        // Sagas placed onto the battlefield through older paths still receive
        // their normal initial lore counter at the shared entry boundary.
        let requested = selected_chapter.unwrap_or(1).max(0);
        let policy = selected_chapter.map_or(
            SagaChapterPolicy::CrossedThresholds,
            SagaChapterPolicy::ReadAheadSelectedOnly,
        );
        self.put_counters_on_permanent_by_with_saga_policy(
            &source.instance_id,
            "lore",
            requested,
            None,
            policy,
        );
    }

    pub(super) fn enqueue_saga_chapters_for_lore_transition(
        &mut self,
        source: &CardInstance,
        old_count: i32,
        new_count: i32,
        policy: SagaChapterPolicy,
    ) {
        if !type_line_contains(&source.definition.type_line, "Saga") {
            return;
        }
        let chapters = saga_chapters_for_transition(old_count, new_count, policy);
        for chapter in chapters {
            self.record_event(
                "loreCounterAdded",
                Some(source.controller.clone()),
                Some(source.instance_id.clone()),
                json!({
                    "chapter": chapter,
                    "oldCount": old_count,
                    "newCount": new_count,
                }),
            );
            self.enqueue_saga_chapter_triggers(source, chapter);
        }
    }

    pub(super) fn enqueue_saga_chapter_triggers(&mut self, source: &CardInstance, chapter: i32) {
        let rules = source
            .definition
            .rules
            .iter()
            .filter(|rule| {
                triggered_rule_supported(rule)
                    && value_kind(&rule["event"]) == Some("sagaChapterReached")
                    && rule["event"]["chapters"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(integer_value)
                        .any(|value| value == chapter)
            })
            .map(|rule| narrowed_saga_chapter_rule(rule, chapter))
            .collect::<Vec<_>>();
        for rule in rules {
            self.put_triggered_ability_on_stack(source, rule);
        }
    }

    pub(super) fn advance_active_player_sagas(&mut self) {
        let active_player_id = self.state.players[self.state.active_player].id.clone();
        let saga_ids = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| {
                permanent.controller == active_player_id
                    && type_line_contains(&permanent.definition.type_line, "Saga")
            })
            .map(|permanent| permanent.instance_id.clone())
            .collect::<Vec<_>>();
        for instance_id in saga_ids {
            self.put_counters_on_permanent_by(&instance_id, "lore", 1, Some(&active_player_id));
        }
    }

    pub(super) fn completed_sagas_ready_to_sacrifice(&self) -> Vec<String> {
        let sagas_with_pending_final_chapter_abilities = self
            .state
            .stack
            .iter()
            .filter(|stack_object| {
                stack_object.ability_kind.as_deref() == Some("triggeredAbility")
                    && stack_object.ability_rule.as_ref().is_some_and(|rule| {
                        saga_chapter_rule_is_final(&stack_object.card.definition, rule)
                    })
            })
            .map(|stack_object| stack_object.card.instance_id.clone())
            .collect::<BTreeSet<_>>();
        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| {
                saga_final_chapter(&permanent.definition).is_some_and(|final_chapter| {
                    permanent.counters.get("lore").copied().unwrap_or(0) >= final_chapter
                })
            })
            .filter(|permanent| {
                !sagas_with_pending_final_chapter_abilities.contains(&permanent.instance_id)
            })
            .map(|permanent| permanent.instance_id.clone())
            .collect()
    }
}
