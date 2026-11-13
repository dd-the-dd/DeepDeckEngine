//! Primitive sacrifice execution for explicit object collections.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use super::{
    EngineError, GameEngine, RuntimeBinding, StackObject, object_expression_supported,
    player_reference_supported, value_kind,
};

pub(super) fn explicit_object_sacrifice_supported(effect: &Value) -> Option<bool> {
    if value_kind(effect) != Some("sacrificePermanents") || effect.get("objects").is_none() {
        return None;
    }
    Some(
        object_expression_supported(&effect["objects"])
            && player_reference_supported(&effect["player"])
            && effect.get("bind").is_none_or(Value::is_string),
    )
}

impl GameEngine {
    pub(super) fn execute_explicit_object_sacrifice(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("sacrificePermanents") || effect.get("objects").is_none() {
            return Ok(false);
        }

        let Some(sacrificing_player_id) =
            self.resolve_player(&effect["player"], stack_object, bindings)
        else {
            return Ok(true);
        };
        let instance_ids = self
            .resolve_runtime_object_ids(&effect["objects"], stack_object, bindings, decisions)
            .unwrap_or_default();
        let mut seen = BTreeSet::new();
        let mut sacrificed = Vec::new();
        for instance_id in instance_ids {
            if !seen.insert(instance_id.clone()) {
                continue;
            }
            let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                continue;
            };
            let controller = self.state.players[player_index].battlefield[card_index]
                .controller
                .clone();
            if controller != sacrificing_player_id {
                continue;
            }
            if stack_object.controller != sacrificing_player_id
                && self.player_has_battlefield_static_modifier(
                    &sacrificing_player_id,
                    "preventOpponentForcedSacrifice",
                )
            {
                self.record_event(
                    "sacrificePrevented",
                    Some(sacrificing_player_id.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    serde_json::json!({
                        "reason": "opponentSpellOrAbility",
                        "permanentId": instance_id,
                    }),
                );
                continue;
            }
            self.move_permanent_to_graveyard(&instance_id, "sacrificed")?;
            sacrificed.push(instance_id);
        }
        if let Some(binding_id) = effect["bind"].as_str() {
            bindings.insert(binding_id.to_string(), RuntimeBinding::Objects(sacrificed));
        }
        Ok(true)
    }
}
