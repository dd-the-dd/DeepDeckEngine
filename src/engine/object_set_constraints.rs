//! Constraints over a chosen set of permanents that no single candidate
//! test expresses (CR 601.2c: the targets are chosen together, and their
//! legality as a set is checked again on resolution, CR 608.2b).
//!
//! `samePermanentController`: `two target creatures controlled by the same
//! player` (CR 108.4). Every chosen object is a permanent on the battlefield
//! and all of them have one controller. Read where each permanent is now; a
//! member that left the battlefield fails the constraint, as a member of
//! `distinctPermanentControllers` does.

use super::{GameEngine, TargetRef};

/// The IR kind of the same-controller set constraint.
pub(super) const SAME_PERMANENT_CONTROLLER: &str = "samePermanentController";

/// Whether a constraint of this module is well formed: the same-controller
/// constraint has no parameter.
pub(super) fn object_set_constraint_supported(kind: Option<&str>) -> bool {
    kind == Some(SAME_PERMANENT_CONTROLLER)
}

impl GameEngine {
    /// CR 108.4: every member of `selection` is a permanent and all of them
    /// share one controller. An empty selection states no controller and
    /// holds vacuously; the slot's cardinality owns its size.
    pub(super) fn selection_has_same_permanent_controller(&self, selection: &[TargetRef]) -> bool {
        let mut controller: Option<&str> = None;
        selection.iter().all(|target| {
            let TargetRef::Permanent { instance_id } = target else {
                return false;
            };
            let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
                return false;
            };
            let current = self.state.players[player_index].battlefield[card_index]
                .controller
                .as_str();
            match controller {
                None => {
                    controller = Some(current);
                    true
                }
                Some(first) => first == current,
            }
        })
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_set_constraints_tests.rs"]
mod tests;
