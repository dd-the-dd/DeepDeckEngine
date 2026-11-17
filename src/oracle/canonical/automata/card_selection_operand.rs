use super::super::{MachineResult, OracleTokenCursor};
use super::card_collection_operand::{
    CardCollectionReferenceNode, CollectionOperandAttempt, try_recognize_card_collection_reference,
};
use super::{
    NumericExpression, ObjectKind, ObjectPredicate, RecognizedEffectFragment,
    recognize_numeric_expression, recognize_object_query_before_separator,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardSelectionQuantityNode {
    Exactly(NumericExpression),
    UpTo(NumericExpression),
    AnyNumber,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardSelectionOperandNode {
    pub quantity: CardSelectionQuantityNode,
    pub predicate: Option<ObjectPredicate>,
    pub from: CardCollectionReferenceNode,
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
        let quantity = match trial.peek().map(str::to_ascii_lowercase).as_deref() {
            Some("a" | "an") => {
                let article = trial.peek().unwrap().to_owned();
                trial.consume(&article)?;
                CardSelectionQuantityNode::Exactly(NumericExpression::from(1))
            }
            Some("any") => {
                trial.consume_phrase(&["any", "number"])?;
                CardSelectionQuantityNode::AnyNumber
            }
            Some("up") => {
                trial.consume_phrase(&["up", "to"])?;
                CardSelectionQuantityNode::UpTo(recognize_numeric_expression(&mut trial, None)?)
            }
            _ => {
                CardSelectionQuantityNode::Exactly(recognize_numeric_expression(&mut trial, None)?)
            }
        };
        trial.enter("collection.selection.predicate-or-source");
        let predicate = if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
        {
            trial.consume("of")?;
            committed = true;
            None
        } else {
            let query = recognize_object_query_before_separator(&mut trial)?;
            trial.require(
                query.controller.is_none(),
                "an unscoped predicate over the bound card collection",
            )?;
            trial.require(
                query.predicate.has_domain(ObjectKind::Card)
                    && query.predicate.supports_domain(ObjectKind::Card),
                "an explicit card-domain predicate",
            )?;
            trial.enter("collection.selection.source-relation");
            trial.consume_phrase(&["from", "among"])?;
            committed = true;
            Some(query.predicate)
        };
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
