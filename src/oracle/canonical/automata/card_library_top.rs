//! The top cards of a library as a pure card-collection operand (CR 401.2).
//! `reveal`, `exile`, `put` and a permission verb consume it independently.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardLibraryTopNode {
    pub count: NumericExpression,
    pub owner: LocalPlayerReference,
}

/// `the top card of your library` / `the top N cards of your library`.
/// Another zone after `of your` is not this operand and stays an explicit
/// failure of the enclosing action.
pub(in crate::oracle::canonical) fn try_recognize_card_library_top(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardLibraryTopNode> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        || !cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("top"))
    {
        return CollectionOperandAttempt::NotClaimed;
    }
    let mut trial = cursor.clone();
    let mut committed = false;
    let result = (|| -> MachineResult<CardLibraryTopNode> {
        trial.enter("collection.library-top.head");
        trial.consume_phrase(&["the", "top"])?;
        committed = true;
        trial.enter("collection.library-top.quantity");
        let count = if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("card"))
        {
            NumericExpression::from(1)
        } else {
            recognize_numeric_expression(&mut trial, None)?
        };
        if let NumericExpression::Atom(NumericAtom::Integer(value)) = &count {
            trial.require(*value > 0, "a positive library-top quantity")?;
        }
        trial.enter("collection.library-top.noun");
        if count == NumericExpression::from(1) {
            trial.consume("card")?;
        } else {
            trial.consume("cards")?;
        }
        trial.enter("collection.library-top.owner");
        trial.consume_phrase(&["of", "your", "library"])?;
        Ok(CardLibraryTopNode {
            count,
            owner: LocalPlayerReference::Controller,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("collection.library-top.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(failure) if committed => CollectionOperandAttempt::FailedCommitted(failure),
        Err(failure) => CollectionOperandAttempt::RejectedCandidate(failure),
    }
}
