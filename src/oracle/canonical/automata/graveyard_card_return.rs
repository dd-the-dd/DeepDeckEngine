use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardZoneChangeAction, CardZoneDestination, ExpressedTargetCardinality, TargetCardSourceZone,
    recognize_target_card_zone_change_effect,
};
use super::{LocalPlayerReference, ObjectPredicate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum OwnedCardZoneNode {
    Graveyard,
    Hand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReturnTargetCardNode {
    pub predicate: ObjectPredicate,
    pub owner: LocalPlayerReference,
    pub from: OwnedCardZoneNode,
    pub to: OwnedCardZoneNode,
}

/// Recognizes one atomic targeted card move. Target characteristics are owned
/// by the shared object-query automaton; this child only adds zone relations.
pub(in crate::oracle::canonical) fn recognize_return_target_card_from_own_graveyard(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ReturnTargetCardNode> {
    let node = recognize_target_card_zone_change_effect(cursor)?;
    cursor.enter("effect.return-card.compatibility");
    cursor.require(
        node.action == CardZoneChangeAction::Move(CardZoneDestination::ControllerHand)
            && node.cardinality == ExpressedTargetCardinality::EXACTLY_ONE
            && node.source_zone == TargetCardSourceZone::ControllerGraveyard,
        "one target card returned from the controller's graveyard to their hand",
    )?;
    Ok(ReturnTargetCardNode {
        predicate: node.predicate,
        owner: LocalPlayerReference::Controller,
        from: OwnedCardZoneNode::Graveyard,
        to: OwnedCardZoneNode::Hand,
    })
}

#[cfg(test)]
#[path = "graveyard_card_return_tests.rs"]
mod tests;
