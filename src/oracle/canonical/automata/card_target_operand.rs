//! Declaration-time targeted card collections. Pure query, never a resolution choice.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardTargetZoneOwner {
    Player(LocalPlayerReference),
    AnyPlayer,
    OnePlayer,
    /// `from an opponent's graveyard`: one opponent's zone (CR 109.5); every
    /// declared card shares that owner.
    Opponent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardTargetOperandNode {
    /// Literal or `X` bounds. When `unbounded` is set the maximum is not
    /// read: `any number of target ... cards` declares zero or more.
    pub cardinality: ExpressedTargetCardinality,
    pub predicate: ObjectPredicate,
    pub zone: CardZoneName,
    pub owner: CardTargetZoneOwner,
    /// `another target ... card`: the ability's source card is not a
    /// candidate (CR 109.5), even after it moved to that zone.
    pub exclude_source: bool,
    /// `any number of target ...` (CR 601.2c): no upper bound.
    pub unbounded: bool,
}

impl CardTargetOperandNode {
    /// The shared count node of this declaration.
    pub(in crate::oracle::canonical) fn count(&self) -> TargetCount {
        if self.unbounded {
            return TargetCount {
                minimum: NumericExpression::Atom(self.cardinality.minimum),
                maximum: TargetCountBound::Unbounded,
            };
        }
        TargetCount::from_atoms(self.cardinality)
    }
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
        | ObjectPredicate::ChosenCreatureType
        | ObjectPredicate::ChosenColor => false,
        ObjectPredicate::Intersection(items) | ObjectPredicate::Union(items) => {
            items.iter().all(declaration_predicate_is_closed)
        }
        ObjectPredicate::Not(item) => declaration_predicate_is_closed(item),
        ObjectPredicate::Qualifier(qualifier) => !qualifier.uses_x(),
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
        // The shared declared-count child: `target`, `another target`, `N
        // target`, `up to N target`, `X target`, `any number of target`.
        let mut exclude_source = false;
        let mut unbounded = false;
        let cardinality = if trial
            .peek()
            .is_some_and(|s| s.eq_ignore_ascii_case("target"))
        {
            ExpressedTargetCardinality::EXACTLY_ONE
        } else if trial
            .peek()
            .is_some_and(|s| s.eq_ignore_ascii_case("another"))
        {
            trial.consume("another")?;
            exclude_source = true;
            ExpressedTargetCardinality::EXACTLY_ONE
        } else {
            let count = recognize_target_count(&mut trial)?;
            if count.is_unbounded() {
                unbounded = true;
                let NumericExpression::Atom(minimum) = count.minimum else {
                    return Err(trial.fail("a literal lower bound for an unbounded card target"));
                };
                ExpressedTargetCardinality {
                    minimum,
                    // Never read while `unbounded` is set.
                    maximum: minimum,
                }
            } else {
                count
                    .atoms()
                    .ok_or_else(|| trial.fail("a literal or X count of declared card targets"))?
            }
        };
        if trial
            .peek()
            .is_some_and(|s| s.eq_ignore_ascii_case("other"))
        {
            trial.consume("other")?;
            exclude_source = true;
        }
        trial.consume("target")?;
        trial.enter("card-target.query");
        let quota = if unbounded {
            NumericExpression::Atom(NumericAtom::VariableX)
        } else {
            NumericExpression::Atom(cardinality.maximum)
        };
        let query =
            recognize_object_noun_query(&mut trial, ObjectQueryContext::shared_quota(&quota))?
                .into_object_query(&mut trial)?;
        trial.require(
            query.predicate.is_in_domain(ObjectKind::Card) && query.controller.is_none(),
            "a card-domain predicate separate from its source zone owner",
        )?;
        trial.require(
            crate::oracle::canonical::effects::domain_filter_is_representable(&query.predicate),
            "a target filter preserving Boolean domain identities",
        )?;
        // An explicit `from ZONE` origin commits the card-target reading:
        // the declaration is in that zone, never a battlefield permanent.
        // `from ZONE` and the location tail `in ZONE` (`target instant card
        // in your graveyard`) state the same declared zone.
        let relation = trial
            .peek()
            .filter(|raw| raw.eq_ignore_ascii_case("from") || raw.eq_ignore_ascii_case("in"))
            .map(str::to_owned);
        committed = relation.is_some();
        trial.enter("card-target.query-capability");
        trial.require(
            declaration_predicate_is_closed(&query.predicate),
            "a proved card-declaration predicate (no AnyZone P/T, color or resolution numeric context)",
        )?;
        trial.enter("card-target.source-relation");
        trial.consume(relation.as_deref().unwrap_or("from"))?;
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
            Some("an")
                if trial.peek_ahead(1).is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("opponent's")
                        || raw.eq_ignore_ascii_case("opponent\u{2019}s")
                }) =>
            {
                trial.consume("an")?;
                let possessive = trial.peek().unwrap_or("opponent's").to_owned();
                trial.consume(&possessive)?;
                (CardTargetZoneOwner::Opponent, false)
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
            exclude_source,
            unbounded,
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
