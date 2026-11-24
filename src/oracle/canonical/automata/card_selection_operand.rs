use super::super::{MachineResult, OracleTokenCursor};
use super::card_collection_operand::{
    CardCollectionReferenceNode, CollectionOperandAttempt, try_recognize_card_collection_reference,
};
use super::{
    ChosenObjectQuantity, NumericExpression, ObjectDeterminer, ObjectKind, ObjectPredicate,
    ObjectQueryContext, RecognizedEffectFragment, recognize_object_determiner,
    recognize_object_noun_query,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardSelectionQuantityNode {
    Exactly(NumericExpression),
    UpTo(NumericExpression),
    AnyNumber,
    /// Every member, with no player choice: `all creature cards from among them`
    /// or `all cards revealed this way`. `via` names the verb of the producing
    /// action when the text states it; the lowerer checks it against the scope.
    /// `except_cast` is the relative clause `that weren't cast [this way]`:
    /// the members a preceding resolution-time cast of the same program did
    /// not cast (CR 608.2c). The lowerer proves that cast action exists.
    All {
        via: Option<CardCollectionVerb>,
        except_cast: bool,
    },
}

/// The action verb that produced a collection and is restated as `VERBed this way`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardCollectionVerb {
    Revealed,
    Exiled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardSelectionOperandNode {
    pub quantity: CardSelectionQuantityNode,
    pub predicate: Option<ObjectPredicate>,
    pub from: CardCollectionReferenceNode,
}

/// `that weren't cast [this way]` / `not cast this way` after a restated
/// collection: the members a preceding cast of the program left uncast.
/// Pure lookahead first; the clause is consumed only when it is complete.
pub(in crate::oracle::canonical) fn recognize_uncast_relative_clause(
    trial: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    let negated_past = |raw: &str| {
        ["weren't", "weren\u{2019}t", "wasn't", "wasn\u{2019}t"]
            .iter()
            .any(|form| raw.eq_ignore_ascii_case(form))
    };
    fn word(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    }
    let relative = word(trial, 0, "that")
        && trial.peek_ahead(1).is_some_and(negated_past)
        && word(trial, 2, "cast");
    // The reduced form always restates its producer: `not cast this way`.
    let reduced = word(trial, 0, "not")
        && word(trial, 1, "cast")
        && word(trial, 2, "this")
        && word(trial, 3, "way");
    if !relative && !reduced {
        return Ok(false);
    }
    trial.enter("collection.selection.uncast-relative");
    if relative {
        trial.consume("that")?;
        let negation = trial.peek().unwrap_or_default().to_owned();
        trial.consume(&negation)?;
    } else {
        trial.consume("not")?;
    }
    trial.consume("cast")?;
    if word(trial, 0, "this") && word(trial, 1, "way") {
        trial.consume_phrase(&["this", "way"])?;
    }
    Ok(true)
}

/// `cards exiled|revealed this way` without a determiner: the bare plural
/// restates the whole producing collection, like `all cards VERBed this way`.
fn bare_this_way_collection_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("revealed") || raw.eq_ignore_ascii_case("exiled")
        })
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        && cursor
            .peek_ahead(3)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("way"))
}

/// Whether `of` at the cursor introduces a bound collection (`of them`,
/// `of those cards`) rather than a quantified noun phrase. Pure lookahead.
fn of_names_collection_reference(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut probe = cursor.clone();
    if probe.consume("of").is_err() {
        return false;
    }
    matches!(
        try_recognize_card_collection_reference(&mut probe),
        CollectionOperandAttempt::Recognized(_)
    )
}

fn recognize_this_way_verb(trial: &mut OracleTokenCursor<'_>) -> MachineResult<CardCollectionVerb> {
    trial.enter("collection.selection.this-way");
    let verb = if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("revealed"))
    {
        trial.consume("revealed")?;
        CardCollectionVerb::Revealed
    } else {
        trial.consume("exiled")?;
        CardCollectionVerb::Exiled
    };
    trial.consume_phrase(&["this", "way"])?;
    Ok(verb)
}

pub(in crate::oracle::canonical) type CardSelectionOperandAttempt =
    CollectionOperandAttempt<CardSelectionOperandNode>;

/// Quantity and query are independent children. No declaration or binding is minted here.
pub(in crate::oracle::canonical) fn try_recognize_card_selection_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> CardSelectionOperandAttempt {
    let mut trial = cursor.clone();
    let mut committed = false;
    let result = (|| -> MachineResult<CardSelectionOperandNode> {
        trial.enter("collection.selection.quantity");
        if bare_this_way_collection_starts(&trial) {
            // `cards exiled this way [that weren't cast]`: the restated scope.
            trial.consume("cards")?;
            let via = recognize_this_way_verb(&mut trial)?;
            committed = true;
            let except_cast = recognize_uncast_relative_clause(&mut trial)?;
            return Ok(CardSelectionOperandNode {
                quantity: CardSelectionQuantityNode::All {
                    via: Some(via),
                    except_cast,
                },
                predicate: None,
                from: CardCollectionReferenceNode::Antecedent,
            });
        }
        // The shared object determiner reads the quantity; a phrase without
        // a selection determiner is not this operand (no numeric misread).
        let quantity = match recognize_object_determiner(&mut trial)? {
            Some(ObjectDeterminer::Chosen {
                quantity,
                another: false,
            }) => match quantity {
                ChosenObjectQuantity::Exactly(count) => CardSelectionQuantityNode::Exactly(count),
                ChosenObjectQuantity::UpTo(count) => CardSelectionQuantityNode::UpTo(count),
                ChosenObjectQuantity::AnyNumber => CardSelectionQuantityNode::AnyNumber,
            },
            Some(ObjectDeterminer::Universal { each: false }) => CardSelectionQuantityNode::All {
                via: None,
                except_cast: false,
            },
            _ => {
                trial.enter("collection.selection.quantity.determiner");
                return Err(trial.fail(
                    "a selection quantity: `a`, a count, `up to` a count, `any number`, or `all`",
                ));
            }
        };
        trial.enter("collection.selection.predicate-or-source");
        // `put one into your hand and the other into your graveyard`: a bare
        // count before the destination names that many members of the
        // observed collection (the elided `of them`).
        if matches!(
            quantity,
            CardSelectionQuantityNode::Exactly(_) | CardSelectionQuantityNode::UpTo(_)
        ) && trial.peek().is_some_and(|raw| {
            ["into", "onto", "on"]
                .iter()
                .any(|relation| raw.eq_ignore_ascii_case(relation))
        }) {
            trial.enter("collection.selection.elided-collection");
            return Ok(CardSelectionOperandNode {
                quantity,
                predicate: None,
                from: CardCollectionReferenceNode::Antecedent,
            });
        }
        let mut quantity = quantity;
        let mut this_way_source = false;
        let mut zone_source = false;
        let predicate = if matches!(quantity, CardSelectionQuantityNode::All { .. })
            && trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
            && trial.peek_ahead(1).is_some_and(|raw| {
                raw.eq_ignore_ascii_case("revealed") || raw.eq_ignore_ascii_case("exiled")
            }) {
            trial.consume("cards")?;
            this_way_source = true;
            let via = Some(recognize_this_way_verb(&mut trial)?);
            committed = true;
            quantity = CardSelectionQuantityNode::All {
                via,
                except_cast: recognize_uncast_relative_clause(&mut trial)?,
            };
            None
        } else if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
            && !(matches!(quantity, CardSelectionQuantityNode::AnyNumber)
                && !of_names_collection_reference(&trial))
        {
            trial.consume("of")?;
            committed = true;
            None
        } else {
            // `any number of QUERY cards from among them`: `of` introduces
            // the quantified noun phrase, not a collection reference.
            if matches!(quantity, CardSelectionQuantityNode::AnyNumber)
                && trial
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
            {
                trial.consume("of")?;
            }
            // A comma continues a coordinated type list (`an artifact,
            // instant, or sorcery card`); any other comma is the parent's.
            let query = recognize_object_noun_query(&mut trial, ObjectQueryContext::ORDINARY)?;
            trial.require(
                query.relations().is_empty(),
                "an unscoped predicate over the bound card collection",
            )?;
            trial.require(
                query.predicate().is_in_domain(ObjectKind::Card),
                "an explicit card-domain predicate",
            )?;
            trial.enter("collection.selection.source-relation");
            if matches!(quantity, CardSelectionQuantityNode::All { .. })
                && trial.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("revealed") || raw.eq_ignore_ascii_case("exiled")
                })
            {
                this_way_source = true;
                let via = Some(recognize_this_way_verb(&mut trial)?);
                quantity = CardSelectionQuantityNode::All {
                    via,
                    except_cast: recognize_uncast_relative_clause(&mut trial)?,
                };
            } else if trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
                && trial
                    .peek_ahead(1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
            {
                // `from it`: the pronoun names a collection only through a
                // singular zone noun (`their hand`); the lowerer proves it.
                trial.consume_phrase(&["from", "it"])?;
                zone_source = true;
            } else {
                trial.consume_phrase(&["from", "among"])?;
            }
            committed = true;
            Some(query.predicate().clone())
        };
        if zone_source {
            return Ok(CardSelectionOperandNode {
                quantity,
                predicate,
                from: CardCollectionReferenceNode::Zone,
            });
        }
        if this_way_source {
            // `VERBed this way` restates the antecedent scope itself.
            return Ok(CardSelectionOperandNode {
                quantity,
                predicate,
                from: CardCollectionReferenceNode::Antecedent,
            });
        }
        trial.enter("collection.selection.source");
        let from = match try_recognize_card_collection_reference(&mut trial) {
            CollectionOperandAttempt::Recognized(fragment) => fragment.node,
            CollectionOperandAttempt::FailedCommitted(f)
            | CollectionOperandAttempt::RejectedCandidate(f) => return Err(f),
            CollectionOperandAttempt::NotClaimed => {
                return Err(trial.fail("a bound card collection reference"));
            }
        };
        trial.require(
            from == CardCollectionReferenceNode::Antecedent,
            "the antecedent collection after of/from among",
        )?;
        Ok(CardSelectionOperandNode {
            quantity,
            predicate,
            from,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("collection.selection.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(f) if committed => CollectionOperandAttempt::FailedCommitted(f),
        Err(f) => CollectionOperandAttempt::RejectedCandidate(f),
    }
}

#[cfg(test)]
#[path = "card_selection_operand_tests.rs"]
mod tests;
