//! Stable trigger sources for a batch of state-based deaths.
//!
//! State-based actions first determine the complete lethal set. The engine must
//! therefore remember every active death-trigger source before removing the
//! first permanent; otherwise a source removed early can miss a later death in
//! the same batch.

use std::collections::BTreeSet;

use serde_json::Value;

use super::GameEngine;

pub(super) use super::trigger_multiplicity::CapturedTriggerSource as DeathTriggerSource;

pub(super) struct DeathTriggerSourceSnapshot {
    sources: Vec<DeathTriggerSource>,
    /// Cards in graveyards before the batch: the only graveyard observers
    /// of its deaths and arrivals (CR 603.10a). A card destroyed earlier in
    /// the same batch does not see a later member of it.
    graveyard_observers: BTreeSet<String>,
}

impl DeathTriggerSourceSnapshot {
    pub(super) fn sources(&self) -> &[DeathTriggerSource] {
        &self.sources
    }

    pub(super) fn graveyard_observer_present(&self, instance_id: &str) -> bool {
        self.graveyard_observers.contains(instance_id)
    }
}

pub(super) fn rule_observes_permanent_death(rule: &Value) -> bool {
    super::zone_change_events::rule_observes_zone_change(
        rule,
        super::zone_change_events::ZoneChangeGroup::Death,
    )
}

impl GameEngine {
    pub(super) fn capture_death_trigger_source_snapshot(&self) -> DeathTriggerSourceSnapshot {
        self.death_trigger_snapshot_from_sources(
            self.capture_battlefield_departure_trigger_sources(),
        )
    }

    pub(super) fn death_trigger_snapshot_from_sources(
        &self,
        sources: Vec<DeathTriggerSource>,
    ) -> DeathTriggerSourceSnapshot {
        let graveyard_observers = self
            .state
            .players
            .iter()
            .flat_map(|player| player.graveyard.iter())
            .map(|card| card.instance_id.clone())
            .collect();
        DeathTriggerSourceSnapshot {
            sources,
            graveyard_observers,
        }
    }

    /// Restricts graveyard arrival observers to the cards of `snapshot`
    /// until [`Self::leave_graveyard_observer_frame`] restores the returned
    /// enclosing frame.
    pub(super) fn enter_graveyard_observer_frame(
        &mut self,
        snapshot: &DeathTriggerSourceSnapshot,
    ) -> Option<BTreeSet<String>> {
        self.graveyard_observer_frame
            .replace(snapshot.graveyard_observers.clone())
    }

    pub(super) fn leave_graveyard_observer_frame(&mut self, previous: Option<BTreeSet<String>>) {
        self.graveyard_observer_frame = previous;
    }

    /// Whether a graveyard card observes the event being published: outside
    /// a simultaneous-departure frame every graveyard card does.
    pub(super) fn graveyard_observer_in_frame(&self, instance_id: &str) -> bool {
        self.graveyard_observer_frame
            .as_ref()
            .is_none_or(|frame| frame.contains(instance_id))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "death_trigger_batch_tests.rs"]
mod tests;
