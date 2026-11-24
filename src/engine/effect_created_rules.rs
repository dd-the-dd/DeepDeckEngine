//! Rules a resolving spell or ability creates (CR 603.7, 611.2).
//!
//! Two kinds of created rules live here, each stored in `rule_modifiers`
//! with the creating ability's source and controller and an explicit
//! lifetime:
//!
//! - A created triggered ability (`installCreatedTrigger`, CR 603.7): its
//!   event is the same event pattern a printed trigger has and is matched by
//!   the same family matchers (zone change, combat, cast). Its source is the
//!   source of the ability that created it and its controller that
//!   ability's controller (CR 603.7d-e). `oneShot` is consumed by its first
//!   occurrence (CR 603.7c); otherwise it triggers each time until it ends.
//! - A created continuous effect over static rule modifiers
//!   (`installTemporaryStaticModifiers`, CR 611.2a): the modifiers are the
//!   static modifier vocabulary of permanents, applied from the created
//!   source while the effect lasts. Every static rule consumer reads them
//!   through the one static modifier site collector.
//!
//! Lifetimes: `untilEndOfCurrentTurn` (CR 514.2), `untilNextTurn` of a
//! player (CR 611.2b) and `restOfGame`.
use super::*;

use super::cast_events::{CastEventRecord, CastSubject};
use super::event_model::{CombatEventRecord, PublicationGroup};
use super::static_modifier_site::BattlefieldStaticModifierSite;
use super::trigger_multiplicity::{CapturedOccurrence, TriggerOrigin};
use super::zone_change_events::{ZoneChangeGroup, ZoneChangeRecord};

/// A created triggered ability waiting for its event.
const CREATED_TRIGGER: &str = "effectCreatedTrigger";
/// A created continuous effect made of static rule modifiers.
const CREATED_STATIC_MODIFIERS: &str = "effectCreatedStaticModifiers";
/// The rule field holding what the created trigger captured when it was
/// created; merged into each occurrence's decisions.
const CREATED_DECISIONS: &str = "createdTriggerDecisions";

fn lifetime_supported(duration: &Value) -> bool {
    match value_kind(duration) {
        Some("untilEndOfCurrentTurn" | "restOfGame") => true,
        Some("untilNextTurn") => player_reference_supported(&duration["player"]),
        _ => false,
    }
}

/// The publication groups whose sites hand events to created triggers.
fn zone_change_variant_published(event: &Value) -> bool {
    super::zone_change_events::zone_change_pattern(event)
        .and_then(Result::ok)
        .is_some_and(|pattern| pattern.group == ZoneChangeGroup::Death)
}

fn combat_variant_published(event: &Value) -> bool {
    super::event_model::combat_event_pattern(event)
        .and_then(Result::ok)
        .is_some_and(|pattern| {
            matches!(
                pattern.group,
                PublicationGroup::AttackControlled | PublicationGroup::AttackSource
            )
        })
}

fn cast_variant_published(event: &Value) -> bool {
    value_kind(event) == Some("spellCast")
        && super::cast_events::cast_subject(event) == CastSubject::AnySpell
}

/// Every alternative of a created trigger's event must reach a publication
/// site that offers it to created triggers; another event is unsupported,
/// never silently inert.
fn created_trigger_event_published(event: &Value) -> bool {
    triggered_event_variants(event).iter().all(|variant| {
        zone_change_variant_published(variant)
            || combat_variant_published(variant)
            || cast_variant_published(variant)
    })
}

/// The triggered rule a created trigger installs, before its captures.
fn created_rule(effect: &Value, effects: Value) -> Value {
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": effect["event"].clone(),
        "effects": effects,
    });
    if let Some(condition) = effect.get("eventCondition") {
        rule["eventCondition"] = condition.clone();
    }
    rule
}

/// A temporary static modifier: the battlefield static vocabulary, or the
/// game rule that damage can't be prevented (CR 615.12).
fn temporary_static_modifier_supported(modifier: &Value) -> bool {
    (value_kind(modifier) == Some("eccStaticAbility")
        && modifier["ability"].as_str() == Some("damageCannotBePrevented"))
        || (value_kind(modifier) != Some("eccStaticAbility")
            && battlefield_static_modifier_supported(modifier))
}

/// Validation of the created-rule effects this module executes.
pub(super) fn created_rule_effect_supported(effect: &Value) -> Option<bool> {
    match value_kind(effect) {
        Some("installCreatedTrigger") => Some(
            player_reference_supported(&effect["controller"])
                && effect["oneShot"].is_boolean()
                && value_kind(&effect["duration"]) != Some("restOfGame")
                && lifetime_supported(&effect["duration"])
                && created_trigger_event_published(&effect["event"])
                && triggered_rule_supported(&created_rule(effect, effect["effects"].clone()))
                && effect
                    .get("captureReferences")
                    .is_none_or(super::captured_binding::captured_references_supported),
        ),
        Some("installTemporaryStaticModifiers") => Some(
            lifetime_supported(&effect["duration"])
                && effect["modifiers"].as_array().is_some_and(|modifiers| {
                    !modifiers.is_empty()
                        && modifiers.iter().all(temporary_static_modifier_supported)
                }),
        ),
        _ => None,
    }
}

impl GameEngine {
    /// Executes the created-rule effects; `false` leaves the effect to the
    /// other domains.
    pub(super) fn execute_created_rule_effect(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<bool, EngineError> {
        match value_kind(effect) {
            Some("installCreatedTrigger") => {
                self.install_created_trigger(effect, stack_object, bindings);
                Ok(true)
            }
            Some("installTemporaryStaticModifiers") => {
                self.install_temporary_static_modifiers(effect, stack_object, bindings);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// The lifetime fields of a created rule. `None` when the player whose
    /// next turn ends it no longer takes turns: the rule then lasts until
    /// the end of the game, like any effect waiting for that turn.
    fn created_rule_lifetime(
        &self,
        duration: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Value {
        match value_kind(duration) {
            Some("untilEndOfCurrentTurn") => {
                json!({ "expiresAfterTurn": self.state.turn_number })
            }
            Some("untilNextTurn") => {
                let ends_before_turn = self
                    .resolve_player(&duration["player"], stack_object, bindings)
                    .and_then(|player_id| self.next_turn_number_for_player(&player_id));
                json!({ "endsBeforeTurn": ends_before_turn })
            }
            _ => json!({}),
        }
    }

    /// Whether a created rule still applies this turn.
    fn created_rule_active(&self, modifier: &Value) -> bool {
        let turn = u64::from(self.state.turn_number);
        modifier["expiresAfterTurn"]
            .as_u64()
            .is_none_or(|last| turn <= last)
            && modifier["endsBeforeTurn"]
                .as_u64()
                .is_none_or(|end| turn < end)
    }

    /// Removes created rules whose lifetime ended.
    fn prune_created_rules(&mut self) {
        let expired = self
            .state
            .rule_modifiers
            .iter()
            .map(|modifier| {
                matches!(
                    value_kind(modifier),
                    Some(CREATED_TRIGGER | CREATED_STATIC_MODIFIERS)
                ) && !self.created_rule_active(modifier)
            })
            .collect::<Vec<_>>();
        let mut index = 0;
        self.state.rule_modifiers.retain(|_| {
            let keep = !expired[index];
            index += 1;
            keep
        });
    }

    /// The source of a created rule: the creating ability's source, with
    /// that ability's controller (CR 603.7d-e, 611.2).
    fn created_rule_source(stack_object: &StackObject, controller_id: &str) -> CardInstance {
        let mut source = stack_object.card.clone();
        source.controller = controller_id.to_string();
        source
    }

    fn next_created_rule_id(&self) -> u64 {
        self.state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                matches!(
                    value_kind(modifier),
                    Some(CREATED_TRIGGER | CREATED_STATIC_MODIFIERS)
                )
            })
            .filter_map(|modifier| modifier["createdRuleId"].as_u64())
            .max()
            .map_or(1, |id| id + 1)
    }

    fn install_created_trigger(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) {
        let Some(controller_id) =
            self.resolve_player(&effect["controller"], stack_object, bindings)
        else {
            return;
        };
        self.prune_created_rules();
        // What the trigger's actions read from the creating ability: the
        // declared targets and event objects it captured with their
        // incarnation (CR 603.7c, 400.7).
        let mut captured = serde_json::Map::new();
        if let Some(captures) = effect.get("captureReferences") {
            captured.insert(
                super::captured_binding::CAPTURED_REFERENCES_KEY.to_string(),
                self.capture_delayed_references(captures, stack_object, bindings),
            );
        }
        let mut rule = created_rule(effect, effect["effects"].clone());
        rule[CREATED_DECISIONS] = Value::Object(captured);
        let mut modifier = json!({
            "kind": CREATED_TRIGGER,
            "createdRuleId": self.next_created_rule_id(),
            "controllerId": controller_id,
            "sourceCard": Self::created_rule_source(stack_object, &controller_id),
            "rule": rule,
            "oneShot": effect["oneShot"].as_bool().unwrap_or(true),
        });
        if let Value::Object(lifetime) =
            self.created_rule_lifetime(&effect["duration"], stack_object, bindings)
        {
            for (key, value) in lifetime {
                modifier[key] = value;
            }
        }
        self.state.rule_modifiers.push(modifier.clone());
        self.record_event(
            "createdTriggerInstalled",
            Some(controller_id),
            Some(stack_object.card.instance_id.clone()),
            modifier,
        );
    }

    fn install_temporary_static_modifiers(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) {
        self.prune_created_rules();
        let controller_id = stack_object.controller.clone();
        let mut modifier = json!({
            "kind": CREATED_STATIC_MODIFIERS,
            "createdRuleId": self.next_created_rule_id(),
            "controllerId": controller_id,
            "sourceCard": Self::created_rule_source(stack_object, &controller_id),
            "modifiers": effect["modifiers"].clone(),
        });
        if let Value::Object(lifetime) =
            self.created_rule_lifetime(&effect["duration"], stack_object, bindings)
        {
            for (key, value) in lifetime {
                modifier[key] = value;
            }
        }
        self.state.rule_modifiers.push(modifier.clone());
        self.record_event(
            "temporaryStaticModifiersInstalled",
            Some(controller_id),
            Some(stack_object.card.instance_id.clone()),
            modifier,
        );
    }

    /// The static modifier sites of the created continuous effects still
    /// in force, read with the battlefield sites (CR 611.2a).
    pub(super) fn effect_created_static_modifier_sites(
        &self,
    ) -> Vec<BattlefieldStaticModifierSite> {
        self.state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                value_kind(modifier) == Some(CREATED_STATIC_MODIFIERS)
                    && self.created_rule_active(modifier)
            })
            .filter_map(|modifier| {
                let source =
                    serde_json::from_value::<CardInstance>(modifier["sourceCard"].clone()).ok()?;
                Some(
                    modifier["modifiers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .enumerate()
                        .map(
                            |(modifier_index, static_modifier)| BattlefieldStaticModifierSite {
                                source: source.clone(),
                                rule_index: usize::MAX,
                                modifier_index,
                                modifier: static_modifier.clone(),
                                outer_rule_supported: true,
                                outer_condition: None,
                            },
                        )
                        .collect::<Vec<_>>(),
                )
            })
            .flatten()
            .collect()
    }

    /// The created triggers in force, with their source and rule.
    fn active_created_triggers(&self) -> Vec<(u64, CardInstance, Value, bool)> {
        self.state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                value_kind(modifier) == Some(CREATED_TRIGGER) && self.created_rule_active(modifier)
            })
            .filter_map(|modifier| {
                let source =
                    serde_json::from_value::<CardInstance>(modifier["sourceCard"].clone()).ok()?;
                Some((
                    modifier["createdRuleId"].as_u64()?,
                    source,
                    modifier["rule"].clone(),
                    modifier["oneShot"].as_bool().unwrap_or(true),
                ))
            })
            .collect()
    }

    /// A one-shot created trigger leaves once it triggered (CR 603.7c).
    fn consume_created_triggers(&mut self, ids: &[u64]) {
        if ids.is_empty() {
            return;
        }
        self.state.rule_modifiers.retain(|modifier| {
            value_kind(modifier) != Some(CREATED_TRIGGER)
                || modifier["createdRuleId"]
                    .as_u64()
                    .is_none_or(|id| !ids.contains(&id))
        });
    }

    /// The created triggers matching one occurrence, consuming the one-shot
    /// ones among them.
    fn take_created_triggers(
        &mut self,
        matches: impl Fn(&Self, &CardInstance, &Value) -> bool,
    ) -> Vec<(CardInstance, Value)> {
        let engine: &Self = self;
        let matched = engine
            .active_created_triggers()
            .into_iter()
            .filter(|(_, source, rule, _)| matches(engine, source, rule))
            .collect::<Vec<_>>();
        let consumed = matched
            .iter()
            .filter(|(_, _, _, one_shot)| *one_shot)
            .map(|(id, ..)| *id)
            .collect::<Vec<_>>();
        self.consume_created_triggers(&consumed);
        matched
            .into_iter()
            .map(|(_, source, rule, _)| (source, rule))
            .collect()
    }

    /// The decisions a created trigger captured when it was created, added
    /// to the decisions of its occurrence (`publish_triggered_ability`).
    pub(super) fn merge_created_trigger_decisions(
        rule: &Value,
        decisions: &mut BTreeMap<String, Value>,
    ) {
        if let Some(captured) = rule[CREATED_DECISIONS].as_object() {
            for (key, value) in captured {
                decisions.insert(key.clone(), value.clone());
            }
        }
    }

    /// Publishes the created triggers of one zone change (CR 603.6, 603.10a)
    /// with the occurrence's decisions.
    pub(super) fn publish_created_zone_change_triggers(
        &mut self,
        group: ZoneChangeGroup,
        record: &ZoneChangeRecord<'_>,
        decisions: &BTreeMap<String, Value>,
    ) {
        let triggers = self.take_created_triggers(|engine, source, rule| {
            engine.rule_matches_zone_change(rule, group, record, source)
        });
        for (source, rule) in triggers {
            self.publish_triggered_ability(
                &source,
                rule,
                decisions.clone(),
                None,
                CapturedOccurrence::Live,
                TriggerOrigin::Delayed,
            );
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// Publishes the created triggers of declared attackers: one occurrence
    /// per matching attack record, naming the attacker and the player it
    /// attacks (CR 508.1, 506.2).
    pub(super) fn publish_created_attack_triggers(&mut self, records: &[CombatEventRecord]) {
        let groups = [
            PublicationGroup::AttackControlled,
            PublicationGroup::AttackSource,
        ];
        let mut occurrences = Vec::new();
        for record in records {
            let triggers = self.take_created_triggers(|engine, source, rule| {
                groups
                    .iter()
                    .any(|group| engine.rule_matches_combat_record(rule, *group, record, source))
            });
            for (source, rule) in triggers {
                occurrences.push((
                    source,
                    rule,
                    BTreeMap::from([
                        (
                            "triggeringPermanentId".to_string(),
                            Value::String(record.attacker.clone()),
                        ),
                        (
                            "triggeringPlayerId".to_string(),
                            Value::String(record.defending_player.clone()),
                        ),
                    ]),
                ));
            }
        }
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

    /// The created triggers of one spell cast, for the cast publication's
    /// shared decisions (CR 601.2i).
    pub(super) fn take_created_spell_cast_triggers(
        &mut self,
        record: &CastEventRecord<'_>,
    ) -> Vec<(CardInstance, Value, TriggerOrigin)> {
        self.take_created_triggers(|engine, source, rule| {
            triggered_event_variants(&rule["event"])
                .iter()
                .any(|event| {
                    engine.spell_cast_matches(event, record, source, CastSubject::AnySpell)
                })
        })
        .into_iter()
        .map(|(source, rule)| (source, rule, TriggerOrigin::Delayed))
        .collect()
    }
}
