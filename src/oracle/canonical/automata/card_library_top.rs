//! The top cards of a library as a pure card-collection operand (CR 401.2).
//! `reveal`, `exile`, `put` and a permission verb consume it independently.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

/// The player whose library holds the cards: a contextual player (`your
/// library`) or the player a possessive target head declares (`target
/// opponent's library`, read by the shared target-list child).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardLibraryOwnerNode {
    Player(LocalPlayerReference),
    TargetPlayer(TargetListNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardLibraryTopNode {
    pub count: NumericExpression,
    pub owner: CardLibraryOwnerNode,
}

/// `the top card of your library` / `the top N cards of your library`: the
/// historical contract of consumers whose library is the controller's
/// (manifest, a look permission). A declared library owner fails here.
pub(in crate::oracle::canonical) fn try_recognize_card_library_top(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardLibraryTopNode> {
    try_recognize_card_library_position(cursor, false)
}

/// The library position operand of an action verb: the controller's
/// library or, when `target_owner` is admitted, a declared player's
/// (`the top N cards of target opponent's library`). Another zone after
/// `of` is not this operand and stays an explicit failure of the action.
pub(in crate::oracle::canonical) fn try_recognize_card_library_position(
    cursor: &mut OracleTokenCursor<'_>,
    target_owner: bool,
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
        trial.consume("of")?;
        let owner = if target_owner && starts_possessive_target_player(&trial) {
            // The possessive target head is the shared target-list form:
            // the declared player owns the library (CR 115.1, 401.1).
            CardLibraryOwnerNode::TargetPlayer(recognize_possessive_target_player(&mut trial)?)
        } else {
            trial.consume("your")?;
            CardLibraryOwnerNode::Player(LocalPlayerReference::Controller)
        };
        trial.enter("collection.library-top.zone");
        trial.consume("library")?;
        Ok(CardLibraryTopNode { count, owner })
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
