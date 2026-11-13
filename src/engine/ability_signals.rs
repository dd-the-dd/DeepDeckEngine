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
            && effect.get("payload").is_none_or(Value::is_object),
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
        for player_id in self.resolve_players(&effect["player"], stack_object, bindings) {
            self.record_event(
                "abilityPerformed",
                Some(player_id.clone()),
                Some(stack_object.card.instance_id.clone()),
                json!({ "abilityId": ability_id, "payload": payload }),
            );
            self.enqueue_ability_signal_triggers(&player_id, ability_id);
        }
        Ok(true)
    }

    fn enqueue_ability_signal_triggers(&mut self, player_id: &str, ability_id: &str) {
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .filter(|source| source.controller == player_id)
            .flat_map(|source| {
                source
                    .definition
                    .rules
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    value_kind(event) == Some("abilityPerformed")
                                        && event["abilityId"].as_str() == Some(ability_id)
                                })
                    })
                    .cloned()
                    .map(|rule| (source.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack(&source, rule);
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "ability_signals_tests.rs"]
mod tests;
