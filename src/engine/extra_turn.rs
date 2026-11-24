//! Turn succession with extra turns (CR 500.7). An effect that gives a player
//! an extra turn adds it directly after the current turn; when several extra
//! turns are pending after the same turn, the most recently created one is
//! taken first. The player need not be the active player: `target opponent
//! takes an extra turn after this one` hands the next turn to that opponent.
//! An extra turn is inserted into the turn order, it does not move it: once
//! the extra turns created after a turn have all been taken, play resumes with
//! the player who would have taken the turn after that original turn
//! (CR 500.7, 102.1). That player is recorded as a `turnOrderResume` modifier
//! that follows the chain of extra turns.

use super::*;

const EXTRA_TURN: &str = "extraTurnForPlayer";
const TURN_ORDER_RESUME: &str = "turnOrderResume";

impl GameEngine {
    /// Chooses the active player of the next turn once the current turn has
    /// ended: the player of the most recent extra turn created for the turn
    /// just completed, else the player whose normal turn was interrupted by a
    /// chain of extra turns, else the next live player in turn order. The
    /// caller still advances the turn number.
    pub(super) fn advance_to_next_turn_player(&mut self) {
        let turn_number = u64::from(self.state.turn_number);
        // Removed first: the extra-turn search below keeps modifier indices.
        let resume = self.take_turn_order_resume(turn_number);
        let extra_turn = self.state.rule_modifiers.iter().enumerate().rev().find_map(
            |(modifier_index, modifier)| {
                if value_kind(modifier) != Some(EXTRA_TURN)
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
                    if value_kind(modifier) == Some(EXTRA_TURN)
                        && modifier["afterTurn"].as_u64() == Some(turn_number)
                    {
                        modifier["afterTurn"] = json!(turn_number + 1);
                    }
                }
                // The player due after the interrupted normal turn: inherited
                // when this turn was itself an extra turn of the chain.
                let resume_index =
                    resume.unwrap_or_else(|| self.next_live_player_index(self.state.active_player));
                let resume_player_id = self.state.players[resume_index].id.clone();
                self.state.rule_modifiers.push(json!({
                    "kind": TURN_ORDER_RESUME,
                    "playerId": resume_player_id,
                    "afterTurn": turn_number + 1,
                }));
                self.state.active_player = player_index;
                self.record_event("extraTurnStartedNext", Some(player_id), None, json!({}));
            }
            None => {
                self.state.active_player = match resume {
                    Some(resume_index) => resume_index,
                    None => self.next_live_player_index(self.state.active_player),
                };
            }
        }
    }

    /// Removes the resume marker recorded for the turn just completed and
    /// returns the first live player from the recorded one.
    fn take_turn_order_resume(&mut self, turn_number: u64) -> Option<usize> {
        let position = self.state.rule_modifiers.iter().position(|modifier| {
            value_kind(modifier) == Some(TURN_ORDER_RESUME)
                && modifier["afterTurn"].as_u64() == Some(turn_number)
        })?;
        let marker = self.state.rule_modifiers.remove(position);
        let player_id = marker["playerId"].as_str()?;
        let player_index = self
            .state
            .players
            .iter()
            .position(|player| player.id == player_id)?;
        let live_index = self.first_live_player_from(player_index);
        (!self.state.players[live_index].has_lost).then_some(live_index)
    }
}
