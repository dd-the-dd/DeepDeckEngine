//! Declaration-time targeted card collections. Pure query, never a resolution choice.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardTargetZoneOwner {
    Player(LocalPlayerReference),
    AnyPlayer,
    OnePlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardTargetOperandNode {
    pub cardinality: ExpressedTargetCardinality,
    pub predicate: ObjectPredicate,
    pub zone: CardZoneName,
    pub owner: CardTargetZoneOwner,
}

// Capability proof, not another predicate evaluator. Declaration filters do not
// yet have the checked resolution-time numeric/source or AnyZone P/T interface.
fn declaration_predicate_is_closed(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::NumericComparison { left, right, .. } => {
            *left == SelectorNumericProperty::ManaValue
                && matches!(right.as_ref(), NumericExpression::Atom(_))
        }
        // A declaration cannot read a choice made later, on resolution.
        ObjectPredicate::Color(_)
        | ObjectPredicate::ColorQuantity(_)
        | ObjectPredicate::ChosenCreatureType => false,
        ObjectPredicate::Intersection(items) | ObjectPredicate::Union(items) => {
            items.iter().all(declaration_predicate_is_closed)
        }
        ObjectPredicate::Not(item) => declaration_predicate_is_closed(item),
        ObjectPredicate::IsCommander
        | ObjectPredicate::IsToken
        | ObjectPredicate::CardType(_)
        | ObjectPredicate::Subtype(_)
        | ObjectPredicate::CharacteristicBatch(_)
        | ObjectPredicate::Supertype(_)
        | ObjectPredicate::ExactManaCost(_)
        | ObjectPredicate::HasKeyword(_)
        | ObjectPredicate::Status(_)
        | ObjectPredicate::Designation(_)
        | ObjectPredicate::OwnedByAbilityController { .. }
        | ObjectPredicate::PermanentCard
        | ObjectPredicate::HasCounter(_)
        | ObjectPredicate::Relation(_)
        | ObjectPredicate::ObjectKind(_) => true,
    }
}

pub(in crate::oracle::canonical) fn try_recognize_card_target_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardTargetOperandNode> {
    let mut trial = cursor.clone();
    let mut committed = false;
    let result = (|| -> MachineResult<CardTargetOperandNode> {
        trial.enter("card-target.cardinality");
        let cardinality = if trial
            .peek()
            .is_some_and(|s| s.eq_ignore_ascii_case("target"))
        {
            ExpressedTargetCardinality::EXACTLY_ONE
        } else {
            recognize_expressed_target_cardinality(&mut trial)?
        };
        trial.consume("target")?;
        trial.enter("card-target.query");
        let query = recognize_object_noun_query(
            &mut trial,
            ObjectQueryContext::shared_quota(&NumericExpression::Atom(cardinality.maximum)),
        )?
        .into_object_query(&mut trial)?;
        trial.require(
            query.predicate.is_in_domain(ObjectKind::Card) && query.controller.is_none(),
            "a card-domain predicate separate from its source zone owner",
        )?;
        trial.require(
            crate::oracle::canonical::effects::domain_filter_is_representable(&query.predicate),
            "a target filter preserving Boolean domain identities",
        )?;
        trial.enter("card-target.query-capability");
        trial.require(
            declaration_predicate_is_closed(&query.predicate),
            "a proved card-declaration predicate (no AnyZone P/T, color or resolution numeric context)",
        )?;
        trial.enter("card-target.source-relation");
        trial.consume("from")?;
        committed = true;
        trial.enter("card-target.source-owner");
        let (owner, plural) = match trial.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("your") => {
                trial.consume("your")?;
                (
                    CardTargetZoneOwner::Player(LocalPlayerReference::Controller),
                    false,
                )
            }
            Some("a") => {
                trial.consume("a")?;
                if trial
                    .peek()
                    .is_some_and(|s| s.eq_ignore_ascii_case("single"))
                {
                    trial.consume("single")?;
                }
                (CardTargetZoneOwner::OnePlayer, false)
            }
            Some("any") => {
                trial.consume("any")?;
                (CardTargetZoneOwner::AnyPlayer, false)
            }
            Some("graveyards") => (CardTargetZoneOwner::AnyPlayer, true),
            _ => {
                return Err(
                    trial.fail("an explicit source zone owner, a single zone, or graveyards")
                );
            }
        };
        trial.enter("card-target.source-zone");
        if !plural {
            trial.require(
                !trial
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("graveyards")),
                "a singular card zone after a singular owner/article",
            )?;
        }
        let zone_start = trial.checkpoint();
        let zone = if plural {
            trial.consume("graveyards")?;
            CardZoneName::Graveyard
        } else {
            recognize_card_zone_atom(&mut trial)?
        };
        if zone != CardZoneName::Graveyard {
            let mut diagnostic = trial.clone();
            diagnostic.rollback(zone_start);
            diagnostic.enter("card-target.source-capability");
            return Err(diagnostic.fail(
                "a proved graveyard targeting domain; hidden-zone targets remain unpromoted",
            ));
        }
        Ok(CardTargetOperandNode {
            cardinality,
            predicate: query.predicate,
            zone,
            owner,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("card-target.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(failure) if committed => CollectionOperandAttempt::FailedCommitted(failure),
        Err(failure) => CollectionOperandAttempt::RejectedCandidate(failure),
    }
}

pub(in crate::oracle::canonical) fn try_recognize_target_card_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardActionNode> {
    let mut trial = cursor.clone();
    let Some(verb) = trial
        .peek()
        .filter(|s| matches!(s.to_ascii_lowercase().as_str(), "put" | "return" | "exile"))
        .map(str::to_owned)
    else {
        return CollectionOperandAttempt::NotClaimed;
    };
    if let Err(failure) = trial.consume(&verb) {
        return CollectionOperandAttempt::RejectedCandidate(failure);
    }
    let node = match try_recognize_card_target_operand(&mut trial) {
        CollectionOperandAttempt::Recognized(fragment) => fragment.node,
        CollectionOperandAttempt::FailedCommitted(failure) => {
            return CollectionOperandAttempt::FailedCommitted(failure);
        }
        CollectionOperandAttempt::RejectedCandidate(failure) => {
            return CollectionOperandAttempt::RejectedCandidate(failure);
        }
        CollectionOperandAttempt::NotClaimed => return CollectionOperandAttempt::NotClaimed,
    };
    let destination = match recognize_card_action_destination(&mut trial, &verb) {
        Ok(destination) => destination,
        Err(failure) => return CollectionOperandAttempt::FailedCommitted(failure),
    };
    let end_position = trial.position();
    *cursor = trial;
    CollectionOperandAttempt::Recognized(RecognizedEffectFragment {
        node: CardActionNode::Move {
            cards: CardOperandNode::Targets(node),
            destination,
        },
        end_position,
    })
}

#[cfg(test)]
#[path = "card_target_operand_tests.rs"]
mod tests;
