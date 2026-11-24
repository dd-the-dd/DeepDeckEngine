//! Object-predicate atoms of the shared object query that read combat state
//! or the turn's event log rather than an object's definition.
//!
//! The parser owns their grammar (`blocked creature`, `unblocked creature`)
//! and lowers each one to a typed status atom; every engine consumer of an
//! object filter (target candidates, collections, conditions, event filters)
//! evaluates it here through one implementation. A consumer without the event
//! log (a static selector matched without the engine) refuses these atoms in
//! its validator rather than reading them as false.

use super::{CardInstance, GameEngine};

/// Whether `kind` is a combat block-status atom this module evaluates.
pub(super) fn combat_block_status_kind(kind: &str) -> bool {
    matches!(kind, "isBlocked" | "isUnblockedAttacker")
}

impl GameEngine {
    /// Whether this permanent is an attacking creature in the current combat.
    fn permanent_is_declared_attacker(&self, permanent: &CardInstance) -> bool {
        self.state
            .combat
            .attackers
            .iter()
            .any(|attacker| attacker.attacker_id == permanent.instance_id)
    }

    /// CR 509.1h: an attacking creature becomes blocked once a creature is
    /// declared as its blocker, and it remains blocked for the rest of the
    /// combat even if every creature blocking it leaves combat. The current
    /// block pairs are read first; the turn's `blockerDeclared` events keep
    /// the status of an attacker whose blockers were removed, for the same
    /// incarnation of that attacker only.
    pub(super) fn permanent_is_blocked(&self, permanent: &CardInstance) -> bool {
        if !self.permanent_is_declared_attacker(permanent) {
            return false;
        }
        if self
            .state
            .combat
            .blockers
            .iter()
            .any(|blocker| blocker.attacker_id == permanent.instance_id)
        {
            return true;
        }
        let identity = self.object_identity_value(&permanent.instance_id);
        self.state.events.iter().any(|event| {
            event.turn_number == self.state.turn_number
                && event.kind == "blockerDeclared"
                && event.detail["attackerId"].as_str() == Some(permanent.instance_id.as_str())
                && event
                    .detail
                    .get("attackerIdentity")
                    .is_none_or(|recorded| recorded.is_null() || *recorded == identity)
        })
    }

    /// The block status atoms, read for any consumer of a permanent filter:
    /// `isBlocked` and `isUnblockedAttacker` (CR 509.1h), an attacking
    /// creature that is not blocked. A blocked attacker whose blockers left
    /// combat stays blocked, so it never becomes unblocked. `None` for any
    /// other atom.
    pub(super) fn combat_block_status_matches(
        &self,
        permanent: &CardInstance,
        kind: &str,
    ) -> Option<bool> {
        match kind {
            "isBlocked" => Some(self.permanent_is_blocked(permanent)),
            "isUnblockedAttacker" => Some(
                self.permanent_is_declared_attacker(permanent)
                    && !self.permanent_is_blocked(permanent),
            ),
            _ => None,
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_predicate_eval_tests.rs"]
mod tests;
