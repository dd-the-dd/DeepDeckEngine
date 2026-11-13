//! Direct life-total primitives.
//!
//! Composite drains, optional payments, and multi-player aggregate operations
//! remain in the root dispatcher until they can be decomposed independently.

use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{
    EngineError, GameEngine, RuntimeBinding, StackObject, numeric_expression_supported,
    player_reference_supported, value_kind,
};

/// Validates direct `gainLife` and `loseLife` IR nodes.
///
/// `None` means this module does not own the effect kind.
pub(super) fn direct_life_effect_supported(effect: &Value) -> Option<bool> {
    match value_kind(effect) {
        Some("gainLife" | "loseLife") => Some(
            numeric_expression_supported(&effect["amount"])
                && player_reference_supported(&effect["player"]),
        ),
        _ => None,
    }
}

impl GameEngine {
    /// Executes direct life-total changes and reports whether the effect kind
    /// belongs to this module.
    pub(super) fn execute_direct_life_effect(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        let Some(kind @ ("gainLife" | "loseLife")) = value_kind(effect) else {
            return Ok(false);
        };
        let amount = self
            .runtime_numeric_expression(&effect["amount"], stack_object, bindings, decisions)
            .unwrap_or(0);
        for player_id in self.resolve_players(&effect["player"], stack_object, bindings) {
            if kind == "gainLife" {
                self.gain_life(&player_id, amount)?;
            } else {
                let player_index = self.player_index(&player_id)?;
                self.state.players[player_index].life -= amount;
                self.record_event(
                    "lifeLost",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "amount": amount }),
                );
            }
        }
        Ok(true)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "life_tests.rs"]
mod tests;
