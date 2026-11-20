//! Control history read by echo (CR 702.30a).
//!
//! `echoDue` is set whenever a permanent with an echo trigger comes under a
//! player's control (it enters the battlefield or changes controller). When
//! its controller's upkeep begins, that fact moves into the upkeep window
//! read by the `sourceNeedsEcho` intervening condition and `echoDue` resets,
//! so the condition holds exactly during the first upkeep of the controller
//! that began after the permanent came under their control, both when the
//! trigger is checked and when it resolves.

use super::GameEngine;

pub(super) const ECHO_DUE_FLAG: &str = "echoDue";
pub(super) const ECHO_UPKEEP_FLAG: &str = "echoUpkeepDue";

impl GameEngine {
    /// Called once as the active player's upkeep begins, before its triggers.
    pub(super) fn open_echo_upkeep_window(&mut self) {
        let active_player_id = self.state.players[self.state.active_player].id.clone();
        for permanent in self
            .state
            .players
            .iter_mut()
            .flat_map(|player| player.battlefield.iter_mut())
            .filter(|permanent| permanent.controller == active_player_id)
        {
            if permanent.flags.remove(ECHO_DUE_FLAG).unwrap_or(false) {
                permanent.flags.insert(ECHO_UPKEEP_FLAG.to_string(), true);
            } else {
                permanent.flags.remove(ECHO_UPKEEP_FLAG);
            }
        }
    }
}
