//! Atomic actions over a previously observed card collection. No sentence recipes.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardOperandNode {
    Reference(CardCollectionReferenceNode),
    Selection(CardSelectionOperandNode),
    ZoneCollection(CardZoneCollectionNode),
    Targets(CardTargetOperandNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardActionNode {
    Look {
        count: NumericExpression,
        owner: LocalPlayerReference,
    },
    Reveal(CardOperandNode),
    Move {
        cards: CardOperandNode,
        destination: CardMoveDestinationNode,
    },
}

fn operand(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CardOperandNode> {
    match try_recognize_card_target_operand(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::Targets(fragment.node));
        }
        CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_zone_collection(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::ZoneCollection(fragment.node));
        }
        CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_collection_reference(cursor) {
        CardCollectionReferenceAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::Reference(fragment.node));
        }
        CardCollectionReferenceAttempt::FailedCommitted(failure) => return Err(failure),
        CardCollectionReferenceAttempt::NotClaimed
        | CardCollectionReferenceAttempt::RejectedCandidate(_) => {}
    }
    match try_recognize_card_selection_operand(cursor) {
        CardSelectionOperandAttempt::Recognized(fragment) => {
            Ok(CardOperandNode::Selection(fragment.node))
        }
        CardSelectionOperandAttempt::RejectedCandidate(failure)
        | CardSelectionOperandAttempt::FailedCommitted(failure) => Err(failure),
        CardSelectionOperandAttempt::NotClaimed => {
            Err(cursor.fail("a collection reference or quantified card query"))
        }
    }
}

/// `None` means this child made no claim; candidate failure is transactional at
/// the sequence parent. Ellipsis is admitted only after a preceding move action.
pub(in crate::oracle::canonical) fn recognize_card_action(
    cursor: &mut OracleTokenCursor<'_>,
    move_ellipsis: bool,
) -> MachineResult<Option<CardActionNode>> {
    let Some(head) = cursor.peek() else {
        return Ok(None);
    };
    if head.eq_ignore_ascii_case("look") {
        cursor.enter("card-action.look.source");
        cursor.consume_phrase(&["look", "at", "the", "top"])?;
        let count = if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("card"))
        {
            NumericExpression::from(1)
        } else {
            recognize_numeric_expression(cursor, None)?
        };
        cursor.enter("card-action.look.quantity");
        if let NumericExpression::Atom(NumericAtom::Integer(value)) = &count {
            cursor.require(*value > 0, "a positive observation quantity")?;
        }
        if count == NumericExpression::from(1) {
            cursor.consume("card")?;
        } else {
            cursor.consume("cards")?;
        }
        cursor.consume_phrase(&["of", "your", "library"])?;
        return Ok(Some(CardActionNode::Look {
            count,
            owner: LocalPlayerReference::Controller,
        }));
    }
    if head.eq_ignore_ascii_case("reveal") {
        cursor.consume("reveal")?;
        return operand(cursor).map(|value| Some(CardActionNode::Reveal(value)));
    }
    if head.eq_ignore_ascii_case("exile") {
        cursor.consume("exile")?;
        let cards = operand(cursor)?;
        let destination = recognize_card_action_destination(cursor, "exile")?;
        return Ok(Some(CardActionNode::Move { cards, destination }));
    }
    let explicit_move = head.eq_ignore_ascii_case("put") || head.eq_ignore_ascii_case("return");
    if !explicit_move && !move_ellipsis {
        return Ok(None);
    }
    if explicit_move {
        let verb = head.to_owned();
        cursor.consume(&verb)?;
    }
    let cards = operand(cursor)?;
    let destination = recognize_card_action_destination(cursor, "move")?;
    Ok(Some(CardActionNode::Move { cards, destination }))
}

pub(in crate::oracle::canonical) fn recognize_card_action_destination(
    cursor: &mut OracleTokenCursor<'_>,
    verb: &str,
) -> MachineResult<CardMoveDestinationNode> {
    if verb.eq_ignore_ascii_case("exile") {
        return Ok(CardMoveDestinationNode {
            zone: CardZoneName::Exile,
            owner: CardMoveOwnerNode::EachCardOwner,
            position: None,
            order: None,
            tapped: false,
        });
    }
    cursor.enter("card-action.move.destination");
    match try_recognize_card_move_destination(cursor) {
        CardMoveDestinationAttempt::Recognized(fragment) => Ok(fragment.node),
        CardMoveDestinationAttempt::RejectedCandidate(failure)
        | CardMoveDestinationAttempt::FailedCommitted(failure) => return Err(failure),
        CardMoveDestinationAttempt::NotClaimed => {
            return Err(cursor.fail("a typed card destination"));
        }
    }
}

/// Permission owns one complete sentence, including all its coordinated actions.
/// The bounded cursor borrows original tokens and spans; it never retokenizes.
pub(in crate::oracle::canonical) fn recognize_optional_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectSequenceNode> {
    cursor.enter("program.optional.sentence");
    cursor.consume_phrase(&["you", "may"])?;
    let start = cursor.position();
    let mut end = start;
    let mut parentheses = 0usize;
    let mut quoted = false;
    loop {
        match cursor.peek_ahead(end - start) {
            Some("\"") => quoted = !quoted,
            Some("(") if !quoted => parentheses += 1,
            Some(")") if !quoted => parentheses = parentheses.saturating_sub(1),
            Some(".") if !quoted && parentheses == 0 => break,
            None => return Err(cursor.fail("a complete optional sentence")),
            _ => {}
        }
        end += 1;
    }
    let mut body_cursor =
        OracleTokenCursor::new("optional-sentence-body", cursor.token_slice(start, end + 1));
    let body = recognize_effect_sequence_with_context(
        &mut body_cursor,
        name_links,
        name_failure,
        context,
    )?;
    cursor.advance_to(end)?;
    Ok(body)
}
