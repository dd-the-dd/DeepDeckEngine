//! Lowering of the added-phase action (CR 500.8): the engine's
//! `addCombatPhase` with the phase the added phases follow and whether an
//! additional main phase follows the added combat phase.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_additional_phases(node: &AdditionalPhasesNode) -> Value {
    let after = match node.anchor {
        AddedPhaseAnchor::CurrentPhase => "currentPhase",
        AddedPhaseAnchor::CurrentMainPhase => "currentMainPhase",
        AddedPhaseAnchor::CurrentCombatPhase => "currentCombatPhase",
    };
    json!({
        "kind": "addCombatPhase",
        "after": after,
        "followedByMainPhase": node.followed_by_main_phase,
    })
}
