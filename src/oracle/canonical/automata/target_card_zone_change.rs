use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{ExpressedTargetCardinality, ObjectPredicate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneChangeAction {
    Exile,
    Move(CardZoneDestination),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneDestination {
    ControllerHand,
    TargetOwnerLibraryTop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetCardSourceZone {
    AnyGraveyard,
    SingleGraveyard,
    ControllerGraveyard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetCardZoneChangeEffectNode {
    pub action: CardZoneChangeAction,
    pub cardinality: ExpressedTargetCardinality,
    pub source_zone: TargetCardSourceZone,
    pub predicate: ObjectPredicate,
}

// Compatibility projection only: grammar is owned by the shared card operand
// and movement children. Existing consumers retain their historic AST/IR IDs.
fn legacy_card_predicate(predicate: &ObjectPredicate) -> Option<ObjectPredicate> {
    fn only_types(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::CardType(_) => true,
            ObjectPredicate::Union(children) => {
                !children.is_empty() && children.iter().all(only_types)
            }
            _ => false,
        }
    }
    match predicate {
        ObjectPredicate::ObjectKind(super::ObjectKind::Card) => Some(predicate.clone()),
        ObjectPredicate::Intersection(children) if children.len() == 2 => {
            let characteristic = children.iter().find(|child| only_types(child))?;
            children
                .iter()
                .any(|child| matches!(child, ObjectPredicate::ObjectKind(super::ObjectKind::Card)))
                .then(|| {
                    ObjectPredicate::Intersection(vec![
                        ObjectPredicate::ObjectKind(super::ObjectKind::Card),
                        characteristic.clone(),
                    ])
                })
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn legacy_target_card_move(
    action: &super::CardActionNode,
) -> Option<TargetCardZoneChangeEffectNode> {
    use super::*;
    let CardActionNode::Move {
        cards: CardOperandNode::Targets(target),
        destination,
    } = action
    else {
        return None;
    };
    // The historical node has no source exclusion and no unbounded count;
    // such declarations stay on the shared card-action lowering.
    if target.zone != CardZoneName::Graveyard
        || destination.order.is_some()
        || destination.tapped
        || destination.face_down
        || destination.transformed
        || target.exclude_source
        || target.unbounded
    {
        return None;
    }
    let source_zone = match target.owner {
        CardTargetZoneOwner::Player(LocalPlayerReference::Controller) => {
            TargetCardSourceZone::ControllerGraveyard
        }
        CardTargetZoneOwner::AnyPlayer => TargetCardSourceZone::AnyGraveyard,
        CardTargetZoneOwner::OnePlayer => TargetCardSourceZone::SingleGraveyard,
        _ => return None,
    };
    let action = match (
        destination.zone,
        destination.owner.clone(),
        destination.position,
    ) {
        (CardZoneName::Exile, CardMoveOwnerNode::EachCardOwner, None) => {
            CardZoneChangeAction::Exile
        }
        (CardZoneName::Hand, CardMoveOwnerNode::Player(LocalPlayerReference::Controller), None)
            if source_zone == TargetCardSourceZone::ControllerGraveyard
                && target.cardinality == ExpressedTargetCardinality::EXACTLY_ONE =>
        {
            CardZoneChangeAction::Move(CardZoneDestination::ControllerHand)
        }
        (CardZoneName::Library, CardMoveOwnerNode::EachCardOwner, Some(LibraryPosition::Top))
            if target.cardinality == ExpressedTargetCardinality::EXACTLY_ONE
                && source_zone != TargetCardSourceZone::ControllerGraveyard =>
        {
            CardZoneChangeAction::Move(CardZoneDestination::TargetOwnerLibraryTop)
        }
        _ => return None,
    };
    let source_zone =
        if action == CardZoneChangeAction::Move(CardZoneDestination::TargetOwnerLibraryTop) {
            TargetCardSourceZone::AnyGraveyard
        } else {
            source_zone
        };
    Some(TargetCardZoneChangeEffectNode {
        action,
        cardinality: target.cardinality,
        source_zone,
        predicate: legacy_card_predicate(&target.predicate)?,
    })
}

pub(in crate::oracle::canonical) fn recognize_target_card_zone_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCardZoneChangeEffectNode> {
    match super::try_recognize_target_card_action(cursor) {
        super::CollectionOperandAttempt::Recognized(fragment) => {
            legacy_target_card_move(&fragment.node)
                .ok_or_else(|| cursor.fail("a targeted card move in this compatibility consumer"))
        }
        super::CollectionOperandAttempt::RejectedCandidate(failure)
        | super::CollectionOperandAttempt::FailedCommitted(failure) => Err(failure),
        super::CollectionOperandAttempt::NotClaimed => Err(cursor.fail("a targeted card movement")),
    }
}

pub(in crate::oracle::canonical) enum TargetCardZoneChangeParseAttempt {
    NotClaimed,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(TargetCardZoneChangeEffectNode),
}

pub(in crate::oracle::canonical) fn try_recognize_exile_target_card_zone_change_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> TargetCardZoneChangeParseAttempt {
    try_recognize_target_card_zone_change_effect_with_verb(cursor, super::VerbLemma::Exile)
}

/// Transactional claim of a declared card-target move headed by the base
/// form of `verb` (an imperative instruction). The
/// card-target operand owns its cardinality (`target`, `N target`, `up to N
/// target`) and commits only after its source zone relation; any earlier
/// rejection leaves the cursor for the permanent zone-change families.
pub(in crate::oracle::canonical) fn try_recognize_target_card_zone_change_effect_with_verb(
    cursor: &mut OracleTokenCursor<'_>,
    verb: super::VerbLemma,
) -> TargetCardZoneChangeParseAttempt {
    if !cursor
        .peek()
        .is_some_and(|raw| super::is_verb_form(raw, verb, super::VerbForm::Base))
    {
        return TargetCardZoneChangeParseAttempt::NotClaimed;
    }
    let mut trial = cursor.clone();
    match super::try_recognize_target_card_action(&mut trial) {
        super::CollectionOperandAttempt::Recognized(fragment) => {
            match legacy_target_card_move(&fragment.node) {
                Some(node) => {
                    *cursor = trial;
                    TargetCardZoneChangeParseAttempt::Parsed(node)
                }
                None => TargetCardZoneChangeParseAttempt::RejectedCandidate(
                    trial.fail("a compatible targeted card movement"),
                ),
            }
        }
        super::CollectionOperandAttempt::RejectedCandidate(failure) => {
            TargetCardZoneChangeParseAttempt::RejectedCandidate(failure)
        }
        super::CollectionOperandAttempt::FailedCommitted(failure) => {
            TargetCardZoneChangeParseAttempt::FailedCommitted(failure)
        }
        super::CollectionOperandAttempt::NotClaimed => TargetCardZoneChangeParseAttempt::NotClaimed,
    }
}

#[cfg(test)]
#[path = "target_card_zone_change_tests.rs"]
mod tests;
