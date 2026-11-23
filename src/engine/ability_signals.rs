//! Generic named-ability completion signals.
//!
//! The parser emits this atomic primitive after the independent action
//! primitives complete. Rule text and catalog grammar are never interpreted
//! here; the engine only publishes the typed identity supplied by canonical IR.

use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{
    EngineError, GameEngine, RuntimeBinding, StackObject, player_reference_supported,
    triggered_event_variants, triggered_rule_supported, value_kind,
};

pub(super) fn ability_signal_effect_supported(effect: &Value) -> Option<bool> {
    if value_kind(effect) != Some("emitAbilitySignal") {
        return None;
    }
    Some(
        effect["abilityId"]
            .as_str()
            .is_some_and(|ability_id| !ability_id.trim().is_empty())
            && player_reference_supported(&effect["player"])
            && effect.get("payload").is_none_or(Value::is_object)
            && effect
                .get("object")
                .is_none_or(super::object_context::object_context_reference_supported),
    )
}

impl GameEngine {
    pub(super) fn execute_ability_signal_effect(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("emitAbilitySignal") {
            return Ok(false);
        }
        let Some(ability_id) = effect["abilityId"].as_str() else {
            return Err(EngineError::new("ability signal requires abilityId"));
        };
        let payload = effect.get("payload").cloned().unwrap_or_else(|| json!({}));
        let object = effect
            .get("object")
            .map(|reference| {
                self.ability_signal_object_descriptor(reference, stack_object, bindings)
                    .ok_or_else(|| {
                        EngineError::new(
                            "ability signal object has no available current or captured context",
                        )
                    })
            })
            .transpose()?;
        for player_id in self.resolve_players(&effect["player"], stack_object, bindings) {
            let mut detail = json!({ "abilityId": ability_id, "payload": payload });
            let object_id = if let Some((id, descriptor)) = &object {
                detail["object"] = descriptor.clone();
                id.clone()
            } else {
                stack_object.card.instance_id.clone()
            };
            self.record_event(
                "abilityPerformed",
                Some(player_id.clone()),
                Some(object_id),
                detail,
            );
            self.enqueue_ability_signal_triggers(&player_id, ability_id);
        }
        Ok(true)
    }

    /// The acting player is an event attribute: each observer's `player`
    /// reference decides whether it observes that player (CR 603.2).
    fn enqueue_ability_signal_triggers(&mut self, player_id: &str, ability_id: &str) {
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(&source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    value_kind(event) == Some("abilityPerformed")
                                        && event["abilityId"].as_str() == Some(ability_id)
                                        && self.event_player_reference_matches(
                                            &event["player"],
                                            source,
                                            player_id,
                                        )
                                })
                    })
                    .cloned()
                    .map(|rule| (source.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let decisions = BTreeMap::from([(
            "triggeringPlayerId".to_string(),
            Value::String(player_id.to_string()),
        )]);
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "ability_signals_tests.rs"]
mod tests;
