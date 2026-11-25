//! Publication sites of created triggered abilities (CR 603.7) for the
//! event families a printed trigger already observes: a permanent entering,
//! a dealt-damage record observed by its source pattern, a declared blocker,
//! a player gaining life and the controlled-creatures combat-damage batch.
//!
//! A created trigger is matched by the same family matcher as a printed
//! trigger with the same event (`rule_matches_zone_change`,
//! `rule_matches_damage_record`, `rule_matches_combat_record`, the player
//! relation of `event_player_reference_matches`); its source is the source of
//! the ability that created it, with that ability's controller (CR 603.7d-e).
//! Each site binds the occurrence exactly as the printed site does.
//!
//! An event that reads a resolution decision (`decisionId`) is refused: the
//! decision belongs to the creating ability's resolution and is not captured
//! by the created rule, so matching it later would read an absent value.
use super::damage_events::{DamageEventRecord, DamageGroup, DamageRecipientRecord};
use super::entry_observation::EntryObservation;
use super::event_model::{CombatEventRecord, PublicationGroup};
use super::trigger_multiplicity::{CapturedOccurrence, TriggerOrigin};
use super::zone_change_events::{ZoneChangeGroup, ZoneChangeRecord};
use super::*;

const MAX_DEPTH: usize = 64;

fn reads_resolution_decision(value: &Value, depth: usize) -> bool {
    if depth >= MAX_DEPTH {
        return true;
    }
    match value {
        Value::Object(object) => {
            object.contains_key("decisionId")
                || object
                    .values()
                    .any(|value| reads_resolution_decision(value, depth + 1))
        }
        Value::Array(values) => values
            .iter()
            .any(|value| reads_resolution_decision(value, depth + 1)),
        _ => false,
    }
}

/// `permanentEntered`: an observed permanent entering (CR 603.6a). A
/// created trigger cannot observe its own source entering.
fn entry_variant_published(event: &Value) -> bool {
    value_kind(event) == Some("permanentEntered")
        && super::zone_change_events::zone_change_pattern(event)
            .and_then(Result::ok)
            .is_some_and(|pattern| pattern.group == ZoneChangeGroup::Entry)
}

/// `damageDealtBySource` with a source query, one occurrence per record.
fn source_damage_variant_published(event: &Value) -> bool {
    value_kind(event) == Some("damageDealtBySource")
        && super::damage_events::damage_event_pattern(event)
            .and_then(Result::ok)
            .is_some_and(|pattern| {
                pattern.group == DamageGroup::SourceObservers && !pattern.aggregated()
            })
}

/// A generic `block` pattern observed per declared blocker.
fn blocker_variant_published(event: &Value) -> bool {
    super::event_model::combat_event_pattern(event)
        .and_then(Result::ok)
        .is_some_and(|pattern| pattern.group == PublicationGroup::BlockerDeclared)
}

fn life_gained_variant_published(event: &Value) -> bool {
    value_kind(event) == Some("lifeGained")
        && player_action_events::event_player_reference_supported(&event["player"])
}

fn is_source_controller(player: &Value) -> bool {
    value_kind(player) == Some("controllerOf")
        && matches!(
            value_kind(&player["object"]),
            Some("self" | "abilitySource")
        )
}

/// `one or more creatures you control deal combat damage to a player`.
fn controlled_combat_damage_variant_published(event: &Value) -> bool {
    value_kind(event) == Some("controlledCreaturesCombatDamageToPlayer")
        && event.get("player").is_none_or(is_source_controller)
        && event
            .get("where")
            .is_none_or(event_model::event_subject_filter_supported)
        && event.as_object().is_some_and(|object| {
            object
                .keys()
                .all(|key| matches!(key.as_str(), "kind" | "player" | "where"))
        })
}

/// Whether one alternative of a created trigger's event reaches a
/// publication site of this module.
pub(super) fn created_variant_published(event: &Value) -> bool {
    !reads_resolution_decision(event, 0)
        && (entry_variant_published(event)
            || source_damage_variant_published(event)
            || blocker_variant_published(event)
            || life_gained_variant_published(event)
            || controlled_combat_damage_variant_published(event))
}

impl GameEngine {
    fn publish_created_occurrences(
        &mut self,
        occurrences: Vec<(CardInstance, Value, BTreeMap<String, Value>)>,
    ) {
        for (source, rule, decisions) in occurrences {
            self.publish_triggered_ability(
                &source,
                rule,
                decisions,
                None,
                CapturedOccurrence::Live,
                TriggerOrigin::Delayed,
            );
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// CR 603.6a: created triggers observing a permanent entering, read on
    /// the entering object with the printed site's decisions.
    pub(super) fn publish_created_entry_triggers(
        &mut self,
        entered: &CardInstance,
        observation: EntryObservation,
        decisions: &BTreeMap<String, Value>,
    ) {
        let mut decisions = decisions.clone();
        observation.add_object_decision(&mut decisions);
        let record = ZoneChangeRecord {
            object: entered,
            from: None,
            to: "battlefield",
            reason: None,
        };
        self.publish_created_zone_change_triggers(ZoneChangeGroup::Entry, &record, &decisions);
    }

    /// CR 120.3: created triggers observing one dealt-damage record by its
    /// source pattern, bound as the printed source observers bind it.
    pub(super) fn publish_created_source_damage_triggers(&mut self, record: &DamageEventRecord) {
        if record.amount <= 0 {
            return;
        }
        let triggers = self.take_created_triggers(|engine, source, rule| {
            engine.rule_matches_damage_record(rule, DamageGroup::SourceObservers, record, source)
                == Some(false)
        });
        if triggers.is_empty() {
            return;
        }
        let mut decisions = BTreeMap::from([
            ("damageAmount".to_string(), Value::from(record.amount)),
            (
                "damageSourceId".to_string(),
                Value::String(record.source.clone()),
            ),
        ]);
        match &record.recipient {
            DamageRecipientRecord::Player(player_id) => {
                decisions.insert(
                    "triggeringPlayerId".to_string(),
                    Value::String(player_id.clone()),
                );
                decisions.insert(
                    "triggeringPermanentId".to_string(),
                    Value::String(record.source.clone()),
                );
            }
            DamageRecipientRecord::Permanent(instance_id) => {
                decisions.insert(
                    "triggeringPermanentId".to_string(),
                    Value::String(instance_id.clone()),
                );
            }
        }
        self.publish_created_occurrences(
            triggers
                .into_iter()
                .map(|(source, rule)| (source, rule, decisions.clone()))
                .collect(),
        );
    }

    /// CR 509.1i, 509.3a: created triggers observing a declared blocker,
    /// bound to the blocker as the printed blocker observers are.
    pub(super) fn publish_created_blocker_triggers(
        &mut self,
        record: &CombatEventRecord,
    ) -> Result<(), EngineError> {
        let triggers = self.take_created_triggers(|engine, source, rule| {
            engine.rule_matches_combat_record(
                rule,
                PublicationGroup::BlockerDeclared,
                record,
                source,
            )
        });
        let blocker_id = record.blocker.clone().unwrap_or_default();
        let occurrences = triggers
            .into_iter()
            .map(|(source, rule)| {
                let mut decisions = BTreeMap::from([(
                    "triggeringPermanentId".to_string(),
                    Value::String(blocker_id.clone()),
                )]);
                self.bind_generic_block_counterparts(
                    &rule,
                    PublicationGroup::BlockerDeclared,
                    record,
                    &mut decisions,
                );
                (source, rule, decisions)
            })
            .collect();
        self.publish_created_occurrences(occurrences);
        self.check_quantity_failure()
    }

    /// CR 119.9: created triggers observing a player gaining life, with the
    /// amount the printed `lifeGained` observers bind.
    pub(super) fn publish_created_life_gained_triggers(&mut self, player_id: &str, amount: i32) {
        if amount <= 0 {
            return;
        }
        let triggers = self.take_created_triggers(|engine, source, rule| {
            triggered_event_variants(&rule["event"])
                .iter()
                .any(|event| {
                    value_kind(event) == Some("lifeGained")
                        && engine.event_player_reference_matches(
                            &event["player"],
                            source,
                            player_id,
                        )
                })
        });
        let decisions = BTreeMap::from([
            ("lifeGainedAmount".to_string(), Value::from(amount)),
            (
                "triggeringPlayerId".to_string(),
                Value::String(player_id.to_string()),
            ),
        ]);
        self.publish_created_occurrences(
            triggers
                .into_iter()
                .map(|(source, rule)| (source, rule, decisions.clone()))
                .collect(),
        );
    }

    /// CR 510.2, 603.2c: `one or more creatures you control deal combat
    /// damage to a player` triggers once for each damaged player in one
    /// combat-damage step: a pending occurrence of the same created rule for
    /// that player absorbs later damage records of the step.
    pub(super) fn publish_created_controlled_combat_damage_triggers(
        &mut self,
        damage_source: &CardInstance,
        player_id: &str,
        decisions: &BTreeMap<String, Value>,
    ) {
        let pending = |engine: &Self, source: &CardInstance, rule: &Value| {
            engine.state.stack.iter().any(|object| {
                object.card.instance_id == source.instance_id
                    && object.controller == source.controller
                    && object.ability_rule.as_ref().is_some_and(|pending_rule| {
                        pending_rule["event"] == rule["event"]
                            && pending_rule["effects"] == rule["effects"]
                    })
                    && object
                        .decisions
                        .get("triggeringPlayerId")
                        .and_then(Value::as_str)
                        == Some(player_id)
            })
        };
        let triggers = self.take_created_triggers(|engine, source, rule| {
            triggered_event_variants(&rule["event"])
                .iter()
                .any(|event| {
                    value_kind(event) == Some("controlledCreaturesCombatDamageToPlayer")
                        && source.controller == damage_source.controller
                        && engine.permanent_is_creature(damage_source)
                        && event.get("where").is_none_or(|filter| {
                            engine.permanent_matches_effective_filter(damage_source, filter)
                        })
                })
                && !pending(engine, source, rule)
        });
        self.publish_created_occurrences(
            triggers
                .into_iter()
                .map(|(source, rule)| (source, rule, decisions.clone()))
                .collect(),
        );
    }
}
