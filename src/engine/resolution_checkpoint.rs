//! Resolution snapshots keep the append-only event prefix in the live engine.
//!
//! Copying that prefix for every action makes growing-history simulations
//! quadratic. Rules append events; deterministic-position reset is the sole
//! destructive history writer and rejects an active replacement transaction.
//! Nested rollback restores a snapshot taken inside this same prefix.
use super::GameEngine;

pub(super) struct ResolutionCheckpoint {
    snapshot: GameEngine,
    event_count: usize,
}

impl ResolutionCheckpoint {
    pub(super) fn capture(engine: &mut GameEngine) -> Self {
        // Clone has no callback: restore the history before any rule, chooser
        // or preflight observes the engine. All other engine fields are copied.
        let events = std::mem::take(&mut engine.state.events);
        let snapshot = engine.clone();
        engine.state.events = events;
        Self {
            snapshot,
            event_count: engine.state.events.len(),
        }
    }

    pub(super) fn rollback(mut self, engine: &mut GameEngine) {
        let mut events = std::mem::take(&mut engine.state.events);
        debug_assert!(
            events.len() >= self.event_count,
            "resolution must not erase the pre-transaction event prefix"
        );
        events.truncate(self.event_count);
        self.snapshot.state.events = events;
        *engine = self.snapshot;
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "resolution_checkpoint_tests.rs"]
mod tests;
