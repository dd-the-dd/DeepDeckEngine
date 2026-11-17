//! Static future-casting permissions compose actor, spell query, zone and base policy.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    CardZoneName, PlayerSetReferenceNode, SpellCastQueryNode, recognize_card_zone_atom,
    recognize_player_set_reference, recognize_spell_cast_collection,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastingPermissionActorNode {
    Controller,
    Set(PlayerSetReferenceNode),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CastingBaseCostPolicyNode {
    Printed,
    WithoutManaCost,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastingPermissionNode {
    pub actor: CastingPermissionActorNode,
    pub spells: SpellCastQueryNode,
    pub zone: CardZoneName,
    pub base_cost: CastingBaseCostPolicyNode,
}
pub(in crate::oracle::canonical) enum CastingPermissionRecognition {
    NotClaimed,
    RejectedEnvelope(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(CastingPermissionNode),
}

/// The body is a permission, not an effect executed when the permanent resolves.
/// The child borrows the original token cursor. A complete period terminates the
/// static envelope; conditional, temporary and later sentences are not claimed.
pub(in crate::oracle::canonical) fn try_recognize_casting_permission(
    cursor: &mut OracleTokenCursor<'_>,
) -> CastingPermissionRecognition {
    let mut trial = cursor.clone();
    let mut committed_policy = false;
    let parsed = (|| -> MachineResult<CastingPermissionNode> {
        trial.enter("permission.cast.actor");
        let actor = if trial.peek().is_some_and(|w| w.eq_ignore_ascii_case("you")) {
            trial.consume("you")?;
            CastingPermissionActorNode::Controller
        } else {
            CastingPermissionActorNode::Set(recognize_player_set_reference(&mut trial)?)
        };
        trial.enter("permission.cast.future-allowance");
        trial.consume_phrase(&["may", "cast"])?;
        trial.enter("permission.cast.spell-query");
        let spells = recognize_spell_cast_collection(&mut trial)?;
        trial.enter("permission.cast.zone-relation");
        trial.consume("from")?;
        if !trial
            .peek()
            .is_some_and(|w| w.eq_ignore_ascii_case("exile"))
        {
            trial.consume(match actor {
                CastingPermissionActorNode::Controller => "your",
                CastingPermissionActorNode::Set(_) => "their",
            })?;
        }
        let zone = recognize_card_zone_atom(&mut trial)?;
        trial.enter("permission.cast.base-cost");
        let base_cost = if trial
            .peek()
            .is_some_and(|w| w.eq_ignore_ascii_case("without"))
        {
            // Commitment is explicit semantic selection of a policy grammar,
            // not cursor progress or a diagnostic-state naming convention.
            committed_policy = true;
            trial.consume_phrase(&["without", "paying", "their", "mana", "costs"])?;
            CastingBaseCostPolicyNode::WithoutManaCost
        } else {
            CastingBaseCostPolicyNode::Printed
        };
        Ok(CastingPermissionNode {
            actor,
            spells,
            zone,
            base_cost,
        })
    })();
    let node = match parsed {
        Ok(node) => node,
        Err(failure) if committed_policy => {
            return CastingPermissionRecognition::FailedCommitted(failure);
        }
        Err(_) => return CastingPermissionRecognition::NotClaimed,
    };
    trial.enter("permission.cast.complete-envelope");
    if let Err(failure) = trial.consume(".").and_then(|_| trial.finish()) {
        return CastingPermissionRecognition::RejectedEnvelope(failure);
    }
    *cursor = trial;
    CastingPermissionRecognition::Parsed(node)
}
