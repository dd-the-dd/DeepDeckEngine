//! Turn succession with extra turns (CR 500.7). An effect that gives a player
//! an extra turn adds it directly after the current turn; when several extra
//! turns are pending after the same turn, the most recently created one is
//! taken first. The player need not be the active player: `target opponent
//! takes an extra turn after this one` hands the next turn to that opponent,
//! then turn order continues from them (CR 500.7, 102.1).

use super::*;

impl GameEngine {
    /// Chooses the active player of the next turn once the current turn has
    /// ended: the player of the most recent extra turn created for the turn
    /// just completed, else the next live player in turn order. The caller
    /// still advances the turn number.
    pub(super) fn advance_to_next_turn_player(&mut self) {
        let turn_number = u64::from(self.state.turn_number);
        let extra_turn = self.state.rule_modifiers.iter().enumerate().rev().find_map(
            |(modifier_index, modifier)| {
                if value_kind(modifier) != Some("extraTurnForPlayer")
                    || modifier["afterTurn"].as_u64() != Some(turn_number)
                {
                    return None;
                }
                let player_id = modifier["playerId"].as_str()?;
                let player_index = self
                    .state
                    .players
                    .iter()
                    .position(|player| player.id == player_id && !player.has_lost)?;
                Some((modifier_index, player_index, player_id.to_string()))
            },
        );
        match extra_turn {
            Some((modifier_index, player_index, player_id)) => {
                self.state.rule_modifiers.remove(modifier_index);
                // The older extra turns of the same turn follow this one.
                for modifier in &mut self.state.rule_modifiers {
                    if value_kind(modifier) == Some("extraTurnForPlayer")
                        && modifier["afterTurn"].as_u64() == Some(turn_number)
                    {
                        modifier["afterTurn"] = json!(turn_number + 1);
                    }
                }
                self.state.active_player = player_index;
                self.record_event("extraTurnStartedNext", Some(player_id), None, json!({}));
            }
            None => {
                self.state.active_player = self.next_live_player_index(self.state.active_player);
            }
        }
    }
}
