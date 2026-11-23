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
        .printed_rules()
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
    /// Read ahead limits every chapter event during the entry turn, including
    /// later placements. Use the current lore count, not an earlier choice.
    fn saga_chapter_can_trigger(&self, source: &CardInstance, chapter: i32) -> bool {
        let current = self
            .permanent_position(&source.instance_id)
            .map(|(p, c)| &self.state.players[p].battlefield[c])
            .unwrap_or(source);
        !current
            .flags
            .get("enteredThisTurn")
            .copied()
            .unwrap_or(false)
            || !self.permanent_has_keyword(current, "readAhead")
            || current.counters.get("lore").copied() == Some(chapter)
    }

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

    /// Seed the intrinsic replacement before the entering event is committed.
    pub(super) fn prepare_saga_entry_lore(&self, card: &mut CardInstance) {
        if !type_line_contains(&card.definition.type_line, "Saga") {
            return;
        }
        let selected = card.flags.keys().find_map(|flag| {
            flag.strip_prefix(READ_AHEAD_ENTRY_PREFIX)
                .and_then(|n| n.parse::<i32>().ok())
        });
        card.flags.remove(SAGA_ENTRY_PENDING_FLAG);
        card.flags
            .retain(|flag, _| !flag.starts_with(READ_AHEAD_ENTRY_PREFIX));
        if card.counters.contains_key("lore") {
            return;
        }
        card.counters
            .insert("lore".to_string(), selected.unwrap_or(1).max(0));
        if selected.is_some() {
            card.flags
                .insert("sagaEntryReadAheadPrepared".to_string(), true);
        }
    }

    pub(super) fn initialize_saga_lore_on_entry(&mut self, source: &CardInstance) {
        self.publish_entering_counter_events(source);
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
            if !self.saga_chapter_can_trigger(source, chapter) {
                continue;
            }
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
        if !self.saga_chapter_can_trigger(source, chapter) {
            return;
        }
        let rules = source
            .definition
            .printed_rules()
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
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    pub(super) fn advance_active_player_sagas<P: super::DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<(), super::EngineError> {
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
            self.place_counters(
                &instance_id,
                "lore",
                1,
                super::quantity_replacement::QuantityOrigin::RuleAction,
                Some(&active_player_id),
                provider,
            )?;
        }
        Ok(())
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

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "saga_quantity_tests.rs"]
mod quantity_tests;
