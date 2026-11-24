//! One performer slot for every action. An action whose clause names
//! another player as its subject (`that player creates`, `each opponent
//! shuffles`, `target opponent exiles`) carries `performer`: the engine
//! resolves that player once and runs the ordinary primitive as if that
//! player controlled the instruction, so every choice and every `you` inside
//! the action is the performer's (CR 608.2c, 109.5). A `may` instruction is
//! the ordinary optional wrapper run as the performer's: the performer owns
//! that decision (CR 608.2d). The primitive itself never knows about
//! performers.
//!
//! `chosenPlayer` (`an opponent VERBs`) is a performer chosen on resolution
//! by `chooser` among `players` (CR 608.2d). The choice is made once per
//! resolution under its `id`; every primitive of the action, and of the
//! verbs that continue the same subject, reads the same chosen player.

use super::*;

/// `Some(...)` for an action with a performer: the performer must be a
/// supported singular player (or a player chosen on resolution) and the
/// action without it a supported effect.
pub(super) fn performer_effect_supported(effect: &Value) -> Option<bool> {
    let performer = effect.get("performer")?;
    let mut action = effect.clone();
    action.as_object_mut()?.remove("performer");
    let performer_supported = if value_kind(performer) == Some("chosenPlayer") {
        chosen_player_supported(performer)
    } else {
        singular_player_reference_supported(performer)
    };
    Some(performer_supported && effect_supported(&action))
}

fn chosen_player_supported(performer: &Value) -> bool {
    performer["id"].is_string()
        && singular_player_reference_supported(&performer["chooser"])
        && matches!(
            value_kind(&performer["players"]),
            Some("eachPlayer" | "opponentsOf" | "playersExcept")
        )
        && player_reference_supported(&performer["players"])
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
        // A performer that no longer exists (a player who left the game),
        // or a choice with no candidate, performs nothing.
        let player_id = if value_kind(performer) == Some("chosenPlayer") {
            self.chosen_performer(performer, stack_object, bindings, provider)?
        } else {
            self.resolve_player(performer, stack_object, bindings)
        };
        let Some(player_id) = player_id else {
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

    /// The player a `chosenPlayer` performer names: the binding of an earlier
    /// primitive of the same choice, else the chooser's choice among the
    /// population's players who remain in the game, bound under `id`.
    fn chosen_performer<P: DecisionProvider>(
        &mut self,
        performer: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<Option<String>, EngineError> {
        let Some(id) = performer["id"].as_str() else {
            return Ok(None);
        };
        if let Some(RuntimeBinding::Player(player_id)) = bindings.get(id) {
            return Ok(Some(player_id.clone()));
        }
        let Some(chooser_id) = self.resolve_player(&performer["chooser"], stack_object, bindings)
        else {
            return Ok(None);
        };
        let candidates = self
            .resolve_players(&performer["players"], stack_object, bindings)
            .into_iter()
            .filter(|player_id| {
                self.player_index(player_id)
                    .is_ok_and(|index| !self.state.players[index].has_lost)
            })
            .collect::<Vec<_>>();
        // One candidate is no choice (CR 608.2d still names that player).
        let chosen = if candidates.len() == 1 {
            candidates.into_iter().next()
        } else {
            self.choose_resolution_option(
                stack_object,
                provider,
                &chooser_id,
                candidates,
                "Choose the player who performs the instruction.",
            )?
        };
        if let Some(player_id) = &chosen {
            bindings.insert(id.to_string(), RuntimeBinding::Player(player_id.clone()));
        }
        Ok(chosen)
    }
}
