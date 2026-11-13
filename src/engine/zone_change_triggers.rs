//! Generic one-shot triggers that follow a permanent into a configured zone.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    CardInstance, DecisionProvider, EngineError, GameEngine, RuntimeBinding, StackObject,
    effect_supported, player_reference_supported, target_reference_supported, value_kind,
};

const TRACKED_ZONE_PREFIX: &str = "trackedZoneObject:";

pub(super) fn zone_change_effect_supported(effect: &Value) -> Option<bool> {
    match value_kind(effect) {
        Some("installDelayedZoneChangeTrigger") => Some(
            target_reference_supported(&effect["trackedObject"])
                && player_reference_supported(&effect["controller"])
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
                    })
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
                let modifier = json!({
                    "kind": "delayedZoneChangeTrigger",
                    "trackedObjectId": object_id,
                    "controllerId": controller_id,
                    "destinations": effect["destinations"],
                    "effects": effect["effects"],
                    "sourceCard": stack_object.card,
                });
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

                let marker = format!("{TRACKED_ZONE_PREFIX}{zone_identity}");
                let mut found = None;
                for player in &mut self.state.players {
                    let zone = match source_zone {
                        "graveyard" => &mut player.graveyard,
                        "exile" => &mut player.exile,
                        _ => unreachable!(),
                    };
                    if let Some(index) = zone.iter().position(|card| {
                        card.instance_id == instance_id
                            && card.flags.get(&marker).copied().unwrap_or(false)
                    }) {
                        found = Some(zone.remove(index));
                        break;
                    }
                }
                let Some(mut card) = found else {
                    return Ok(true);
                };
                clear_tracking_markers(&mut card);

                if let Some(replacement) =
                    self.card_entry_from_zone_replacement(&card.definition, source_zone)
                {
                    let owner_index = self.player_index(&card.owner)?;
                    card.controller = card.owner.clone();
                    match replacement {
                        "exile" => {
                            self.state.players[owner_index].exile.push(card);
                        }
                        "prohibited" if source_zone == "graveyard" => {
                            self.state.players[owner_index].graveyard.push(card)
                        }
                        "prohibited" => self.state.players[owner_index].exile.push(card),
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
                let entered = card.clone();
                self.state.players[controller_index].battlefield.push(card);
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
    ) {
        let matching = self
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| {
                value_kind(modifier) == Some("delayedZoneChangeTrigger")
                    && modifier["trackedObjectId"].as_str() == Some(left.instance_id.as_str())
                    && modifier["destinations"]
                        .as_array()
                        .is_some_and(|destinations| {
                            destinations
                                .iter()
                                .any(|candidate| candidate.as_str() == Some(destination))
                        })
            })
            .cloned()
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return;
        }

        self.state
            .rule_modifiers
            .retain(|modifier| !matching.iter().any(|consumed| consumed == modifier));
        let identity = format!("{}:{}", self.state.turn_number, self.state.events.len());
        let marker = format!("{TRACKED_ZONE_PREFIX}{identity}");
        for player in &mut self.state.players {
            let zone = match destination {
                "graveyard" => &mut player.graveyard,
                "exile" => &mut player.exile,
                _ => continue,
            };
            if let Some(card) = zone
                .iter_mut()
                .find(|card| card.instance_id == left.instance_id)
            {
                clear_tracking_markers(card);
                card.flags.insert(marker.clone(), true);
                break;
            }
        }
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
            decisions.insert(
                "triggeringZoneIdentity".to_string(),
                Value::String(identity.clone()),
            );
            self.put_triggered_ability_on_stack_with_decisions(
                &source,
                json!({
                    "kind": "triggeredAbility",
                    "source": { "kind": "self" },
                    "effects": modifier["effects"],
                }),
                decisions,
            );
        }
    }

    pub(super) fn clear_tracked_zone_identity(card: &mut CardInstance) {
        clear_tracking_markers(card);
    }
}
