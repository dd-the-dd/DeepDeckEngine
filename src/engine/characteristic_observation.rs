//! Read-only observation of a permanent's effective characteristics (CR 613)
//! for harnesses and fixtures. It evaluates the engine's own effective filter,
//! so an observation can never drift from what rules decisions see.
use super::*;

impl GameEngine {
    /// Whether the battlefield permanent `instance_id` matches `filter` after
    /// every layered effect (`colorContains`, `cardTypeContains`,
    /// `subtypeContains`, ...). `None` when no such permanent exists.
    pub fn permanent_matches_effective_filter_by_id(
        &self,
        instance_id: &str,
        filter: &Value,
    ) -> Option<bool> {
        let (player_index, card_index) = self.permanent_position(instance_id)?;
        let permanent = &self.state.players[player_index].battlefield[card_index];
        Some(self.permanent_matches_effective_filter(permanent, filter))
    }
}
