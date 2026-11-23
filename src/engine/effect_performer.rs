//! One performer slot for every action. An action whose clause names
//! another player as its subject (`that player creates`, `each opponent
//! shuffles`) carries `performer`: the engine resolves that player once and
//! runs the ordinary primitive as if that player controlled the instruction,
//! so every choice and every `you` inside the action is the performer's
//! (CR 608.2c, 109.5). The primitive itself never knows about performers.

use super::*;

/// `Some(...)` for an action with a performer: the performer must be a
/// supported singular player and the action without it a supported effect.
pub(super) fn performer_effect_supported(effect: &Value) -> Option<bool> {
    let performer = effect.get("performer")?;
    let mut action = effect.clone();
    action.as_object_mut()?.remove("performer");
    Some(singular_player_reference_supported(performer) && effect_supported(&action))
}

impl GameEngine {
    /// Runs an action with a performer as that player's instruction. `Ok(false)`
    /// when the effect names no performer.
    pub(super) fn execute_performed_effect<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        let Some(performer) = effect.get("performer") else {
            return Ok(false);
        };
        // A performer that no longer exists (a player who left the game)
        // performs nothing.
        let Some(player_id) = self.resolve_player(performer, stack_object, bindings) else {
            return Ok(true);
        };
        if self
            .player_index(&player_id)
            .is_ok_and(|index| self.state.players[index].has_lost)
        {
            return Ok(true);
        }
        let mut action = effect.clone();
        if let Some(fields) = action.as_object_mut() {
            fields.remove("performer");
        }
        let mut performed = stack_object.clone();
        performed.controller = player_id;
        self.execute_effect_in_batch(
            &action,
            next_effect,
            &performed,
            bindings,
            decisions,
            provider,
        )?;
        Ok(true)
    }
}
