//! Ascend on an instant or sorcery (CR 702.131a): a spell ability meaning
//! "If you control ten or more permanents and you don't have the city's
//! blessing, you get the city's blessing for the rest of the game." It is
//! one effect of the spell's program, performed in printed order, so a later
//! instruction of the same spell reads the designation it may have granted.
//!
//! ```json
//! {"kind": "ascend", "player": {"kind": "controllerOf", "object": {"kind": "self"}}}
//! ```
//!
//! The city's blessing is the same `cityBlessing` player designation the
//! state check of a permanent with ascend grants (CR 702.131b-c).
use super::*;

const ASCEND_EFFECT: &str = "ascend";
/// CR 702.131a: the number of permanents the player must control.
const ASCEND_THRESHOLD: usize = 10;

pub(super) fn ascend_effect_supported(effect: &Value) -> Option<bool> {
    (value_kind(effect) == Some(ASCEND_EFFECT))
        .then(|| player_reference_supported(&effect["player"]))
}

impl GameEngine {
    /// Performs an ascend spell ability. `Ok(false)` when the effect is not
    /// one.
    pub(super) fn execute_ascend_effect(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some(ASCEND_EFFECT) {
            return Ok(false);
        }
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            return Ok(true);
        };
        let controlled = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| permanent.controller == player_id)
            .count();
        let blessed = self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("cityBlessing")
                && modifier["playerId"].as_str() == Some(player_id.as_str())
        });
        if controlled >= ASCEND_THRESHOLD && !blessed {
            self.state.rule_modifiers.push(json!({
                "kind": "cityBlessing",
                "playerId": player_id,
            }));
            self.record_event("cityBlessingGained", Some(player_id), None, json!({}));
        }
        Ok(true)
    }
}
