//! Generic one-shot triggers that follow a permanent into a configured zone.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::trigger_multiplicity;
use super::{
    CardInstance, DecisionProvider, EngineError, GameEngine, RuntimeBinding, StackObject,
    effect_supported, player_reference_supported, target_reference_supported, value_kind,
};

pub(super) const TRACKED_ZONE_PREFIX: &str = "trackedZoneObject:";

pub(super) fn zone_change_effect_supported(effect: &Value) -> Option<bool> {
    match value_kind(effect) {
        Some("installDelayedZoneChangeTrigger") => Some(
            target_reference_supported(&effect["trackedObject"])
                && player_reference_supported(&effect["controller"])
                // `leaves the battlefield` (CR 603.6c): any destination; else
                // the listed public destinations (`dies`: the graveyard).
                && (effect["anyDestination"].as_bool() == Some(true)
                    && effect.get("destinations").is_none()
                    || effect.get("anyDestination").is_none()
                        && effect["destinations"]
                            .as_array()
                            .is_some_and(|destinations| {
                                !destinations.is_empty()
                                    && destinations.iter().all(|destination| {
                                        matches!(destination.as_str(), Some("graveyard" | "exile"))
                                    })
                                    && destinations
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .collect::<BTreeSet<_>>()
                                        .len()
                                        == destinations.len()
                            }))
                // A trigger created `this turn` expires with the turn.
                && effect
                    .get("duration")
                    .is_none_or(|duration| value_kind(duration) == Some("untilEndOfCurrentTurn"))
                && effect
                    .get("captureReferences")
                    .is_none_or(super::captured_binding::captured_references_supported)
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                }),
        ),
        Some("moveTriggeringCard") => Some(
            value_kind(&effect["from"]) == Some("triggeringDestination")
                && effect["to"].as_str() == Some("battlefield")
                && player_reference_supported(&effect["controller"])
                && effect["tapped"].is_boolean(),
        ),
        _ => None,
    }
}

fn clear_tracking_markers(card: &mut CardInstance) {
    card.flags
        .retain(|flag, _| !flag.starts_with(TRACKED_ZONE_PREFIX));
}

impl GameEngine {
    pub(super) fn execute_zone_change_effect<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        match value_kind(effect) {
            Some("installDelayedZoneChangeTrigger") => {
                let Some(object_id) = self
                    .resolve_target(&effect["trackedObject"], stack_object, bindings)
                    .and_then(|target| match target {
                        super::TargetRef::Permanent { instance_id } => Some(instance_id),
                        _ => None,
                    })
                else {
                    return Ok(true);
                };
                let Some(controller_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(true);
                };
                let mut modifier = json!({
                    "kind": "delayedZoneChangeTrigger",
                    "trackedObjectId": object_id,
                    "controllerId": controller_id,
                    "destinations": effect["destinations"],
                    "effects": effect["effects"],
                    "sourceCard": stack_object.card,
                });
                if effect["anyDestination"].as_bool() == Some(true) {
                    modifier["anyDestination"] = Value::Bool(true);
                }
                if effect.get("duration").is_some() {
                    modifier["expiresAfterTurn"] = json!(self.state.turn_number);
                }
                // What the trigger's actions read from the creating ability
                // (CR 603.7c).
                if let Some(captures) = effect.get("captureReferences") {
                    modifier["capturedReferences"] =
                        self.capture_delayed_references(captures, stack_object, bindings);
                }
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "delayedZoneChangeTriggerInstalled",
                    Some(controller_id),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
                Ok(true)
            }
            Some("moveTriggeringCard") => {
                let Some(instance_id) = decisions
                    .get("triggeringPermanentId")
                    .and_then(Value::as_str)
                else {
                    return Ok(true);
                };
                let Some(source_zone) = decisions
                    .get("leftBattlefieldDestination")
                    .and_then(Value::as_str)
                    .filter(|zone| matches!(*zone, "graveyard" | "exile"))
                else {
                    return Ok(true);
                };
                let Some(zone_identity) = decisions
                    .get("triggeringZoneIdentity")
                    .and_then(Value::as_str)
                else {
                    return Ok(true);
                };
                let Some(controller_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(true);
                };

                let found = self
                    .prepare_card_movement_source(instance_id, None)?
                    .filter(|source| {
                        source.zone.name() == source_zone
                            && source.identity.zone_identity == zone_identity
                    })
                    .map(|source| self.take_prepared_movement_card(source))
                    .transpose()?;
                let Some(mut card) = found else {
                    return Ok(true);
                };

                if let Some(replacement) =
                    self.card_entry_from_zone_replacement(&card.definition, source_zone)
                {
                    let owner_index = self.player_index(&card.owner)?;
                    card.controller = card.owner.clone();
                    match replacement {
                        "exile" => {
                            self.commit_movement_identity(
                                &mut card,
                                &super::card_zone_transition::ObjectZone::Exile,
                                super::card_zone_transition::ObjectTransitionReason::Reexile,
                            )?;
                            self.state.players[owner_index].exile.push(card.card);
                        }
                        "prohibited" if source_zone == "graveyard" => {
                            self.state.players[owner_index].graveyard.push(card.card)
                        }
                        "prohibited" => self.state.players[owner_index].exile.push(card.card),
                        _ => {}
                    }
                    self.record_event(
                        "triggeringCardMoveReplaced",
                        Some(controller_id),
                        Some(instance_id.to_string()),
                        json!({ "replacement": replacement, "from": source_zone }),
                    );
                    return Ok(true);
                }

                let controller_index = self.player_index(&controller_id)?;
                card.controller = controller_id.clone();
                card.tapped = effect["tapped"].as_bool().unwrap_or(false);
                card.summoning_sick = true;
                card.flags
                    .insert(format!("enteredFrom:{source_zone}"), true);
                self.apply_enter_replacements(&mut card, provider)?;
                if effect["tapped"].as_bool() == Some(true) {
                    card.tapped = true;
                }
                self.commit_movement_identity(
                    &mut card,
                    &super::card_zone_transition::ObjectZone::Battlefield,
                    super::card_zone_transition::ObjectTransitionReason::Move,
                )?;
                let entered = card.clone();
                self.state.players[controller_index]
                    .battlefield
                    .push(card.card);
                self.record_event(
                    "triggeringCardMoved",
                    Some(controller_id),
                    Some(instance_id.to_string()),
                    json!({
                        "from": source_zone,
                        "to": "battlefield",
                        "tapped": effect["tapped"],
                    }),
                );
                self.enqueue_enter_battlefield_triggers(&entered);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub(super) fn enqueue_delayed_zone_change_triggers(
        &mut self,
        left: &CardInstance,
        destination: &str,
        base_decisions: &BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let turn_number = u64::from(self.state.turn_number);
        let matching = self
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                value_kind(modifier) == Some("delayedZoneChangeTrigger")
                    && modifier["trackedObjectId"].as_str() == Some(left.instance_id.as_str())
                    && modifier["expiresAfterTurn"]
                        .as_u64()
                        .is_none_or(|turn| turn == turn_number)
                    && (modifier["anyDestination"].as_bool() == Some(true)
                        || modifier["destinations"]
                            .as_array()
                            .is_some_and(|destinations| {
                                destinations
                                    .iter()
                                    .any(|candidate| candidate.as_str() == Some(destination))
                            }))
            })
            .cloned()
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return Ok(());
        }
        // A trigger that follows the object into a listed public zone finds
        // the card it became there; a `leaves the battlefield` trigger may
        // see it cease to exist (a token) or enter a hidden zone.
        let any_destination_only = matching
            .iter()
            .all(|modifier| modifier["anyDestination"].as_bool() == Some(true));
        let source = self.prepare_card_movement_source(&left.instance_id, None)?;
        let identity = match source {
            Some(source) if source.zone.name() == destination => {
                Some(source.identity.zone_identity)
            }
            _ if any_destination_only => None,
            None => {
                return Err(EngineError::new(
                    "delayed zone observation requires the committed destination object",
                ));
            }
            Some(_) => {
                return Err(EngineError::new(
                    "delayed zone observation has an incorrect destination",
                ));
            }
        };

        self.state
            .rule_modifiers
            .retain(|modifier| !matching.iter().any(|consumed| consumed == modifier));
        for modifier in matching {
            let Ok(mut source) =
                serde_json::from_value::<CardInstance>(modifier["sourceCard"].clone())
            else {
                continue;
            };
            if let Some(controller_id) = modifier["controllerId"].as_str() {
                source.controller = controller_id.to_string();
            }
            let mut decisions = base_decisions.clone();
            if let Some(identity) = &identity {
                decisions.insert(
                    "triggeringZoneIdentity".to_string(),
                    Value::String(identity.clone()),
                );
            }
            if let Some(captured) = modifier.get("capturedReferences") {
                decisions.insert(
                    super::captured_binding::CAPTURED_REFERENCES_KEY.to_string(),
                    captured.clone(),
                );
            }
            self.put_triggered_ability_on_stack_with_origin(
                &source,
                json!({
                    "kind": "triggeredAbility",
                    "source": { "kind": "self" },
                    "effects": modifier["effects"],
                }),
                decisions,
                trigger_multiplicity::TriggerOrigin::Delayed,
            );
            if self.check_quantity_failure().is_err() {
                return self.check_quantity_failure();
            }
        }
        Ok(())
    }

    /// Whether a modifier bound to an object's incarnation (`zoneIdentity`,
    /// recorded when the effect began) still applies to `card`: a permanent
    /// that left the battlefield and returned is a new object the effect
    /// never applied to (CR 400.7, 611.2c). A modifier without the field is
    /// not incarnation-bound.
    pub(super) fn modifier_incarnation_current(modifier: &Value, card: &CardInstance) -> bool {
        modifier
            .get("zoneIdentity")
            .is_none_or(|bound| bound.as_str() == Self::tracked_zone_identity(card))
    }

    /// The incarnation to record on a modifier bound to `card` (`null` for
    /// an object without a tracked incarnation, which then matches only
    /// while it still has none).
    pub(super) fn incarnation_binding(card: &CardInstance) -> Value {
        Self::tracked_zone_identity(card).map_or(Value::Null, |identity| Value::from(identity))
    }

    pub(super) fn tracked_zone_identity(card: &CardInstance) -> Option<&str> {
        card.flags.iter().find_map(|(flag, active)| {
            active
                .then(|| flag.strip_prefix(TRACKED_ZONE_PREFIX))
                .flatten()
        })
    }

    pub(super) fn assign_tracked_zone_identity(card: &mut CardInstance, identity: &str) {
        clear_tracking_markers(card);
        card.flags
            .insert(format!("{TRACKED_ZONE_PREFIX}{identity}"), true);
    }

    pub(super) fn clear_tracked_zone_identity(card: &mut CardInstance) {
        clear_tracking_markers(card);
    }
}
