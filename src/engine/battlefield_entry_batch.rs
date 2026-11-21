//! Commit one battlefield movement before publishing any of its ETB events.

use serde_json::Value;

use super::{CardInstance, GameEngine, value_kind};

/// This candidate is the card being moved, scoped to a moveCards destination.
/// It is deliberately not a generally resolvable player reference.
pub(super) fn destination_uses_each_owner(destination: &Value) -> bool {
    value_kind(destination) == Some("battlefield")
        && value_kind(&destination["player"]) == Some("ownerOf")
        && value_kind(&destination["player"]["object"]) == Some("candidate")
}

impl GameEngine {
    /// Establishes a new battlefield incarnation before any observer can copy
    /// the entering object. Previous graveyard/exile markers are not evidence
    /// of battlefield preparation and are always replaced at a real entry.
    pub(super) fn prepare_battlefield_entry_identity(&mut self, card: &mut CardInstance) {
        self.assign_new_object_incarnation(card)
            .expect("battlefield entry requires a valid object identity reservation");
    }

    /// Identity-aware moveCards entry: reserve/install the complete batch
    /// before publication. No callback or other allocation can stale the
    /// reservation; the established entry observer order is unchanged.
    pub(super) fn commit_identity_checked_battlefield_entries(
        &mut self,
        mut entries: Vec<(usize, CardInstance)>,
    ) -> Result<(), super::EngineError> {
        use super::object_identity::{
            IdentityAssignment, install_object_identities, prepare_object_identities,
        };
        for (player, _) in &entries {
            if *player >= self.state.players.len() {
                return Err(super::EngineError::new(
                    "invalid battlefield destination player",
                ));
            }
        }
        let cards = entries.iter().map(|(_, card)| card).collect::<Vec<_>>();
        let prepared = prepare_object_identities(
            self.state.battlefield_incarnation_sequence,
            &cards,
            IdentityAssignment::NewIncarnation,
        )?;
        let mut cards = entries.iter_mut().map(|(_, card)| card).collect::<Vec<_>>();
        install_object_identities(
            &mut self.state.battlefield_incarnation_sequence,
            &mut cards,
            prepared,
        )?;
        let mut sources = Vec::with_capacity(entries.len());
        for (player, mut card) in entries {
            card.flags.insert("enteredThisTurn".to_string(), true);
            sources.push(card.clone());
            self.state.players[player].battlefield.push(card);
        }
        for source in &sources {
            self.publish_prepared_enter_battlefield_triggers(source);
        }
        Ok(())
    }

    pub(super) fn commit_prepared_battlefield_entries(
        &mut self,
        entries: Vec<(usize, CardInstance)>,
    ) {
        let sources = self.install_prepared_battlefield_entries(entries);
        for source in &sources {
            self.publish_prepared_enter_battlefield_triggers(source);
        }
    }

    /// Shared simultaneous installation. Callers publish only after every
    /// newcomer's object and zone identity are visible to all observers.
    pub(super) fn install_prepared_battlefield_entries(
        &mut self,
        entries: Vec<(usize, CardInstance)>,
    ) -> Vec<CardInstance> {
        let mut sources = Vec::with_capacity(entries.len());
        for (player_index, mut card) in entries {
            card.flags.insert("enteredThisTurn".to_string(), true);
            self.prepare_battlefield_entry_identity(&mut card);
            sources.push(card.clone());
            self.state.players[player_index].battlefield.push(card);
        }
        // Every newcomer is live and already carries its fresh incarnation
        // before any one can observe another's entry and capture a source.
        sources
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "battlefield_incarnation_counter_tests.rs"]
mod incarnation_counter_tests;
