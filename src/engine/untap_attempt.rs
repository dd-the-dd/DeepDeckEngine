//! The shared state transition only: untap-step eligibility stays with its
//! caller. A stun counter replaces a real tapped -> untapped attempt (CR122.1d).
use super::GameEngine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UntapAttemptOutcome {
    Unchanged,
    StunCounterRemoved,
    Untapped,
}

impl GameEngine {
    pub(super) fn attempt_untap_permanent(&mut self, instance_id: &str) -> UntapAttemptOutcome {
        let Some((player, index)) = self.permanent_position(instance_id) else {
            return UntapAttemptOutcome::Unchanged;
        };
        let permanent = &mut self.state.players[player].battlefield[index];
        if permanent.flags.get("phasedOut").copied().unwrap_or(false) || !permanent.tapped {
            return UntapAttemptOutcome::Unchanged;
        }
        let stun = permanent.counters.get("stun").copied().unwrap_or(0);
        if stun > 0 {
            // This same exclusive engine borrow has just observed a positive
            // counter; the shared removal service owns mutation and its event.
            self.remove_counters_from_permanent(instance_id, "stun", 1, "wouldUntap");
            return UntapAttemptOutcome::StunCounterRemoved;
        }
        permanent.tapped = false;
        self.publish_became_untapped_observers(instance_id);
        UntapAttemptOutcome::Untapped
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "untap_attempt_tests.rs"]
mod tests;
