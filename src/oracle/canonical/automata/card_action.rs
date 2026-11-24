//! Atomic actions over a previously observed card collection. No sentence recipes.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardOperandNode {
    Reference(CardCollectionReferenceNode),
    Selection(CardSelectionOperandNode),
    ZoneCollection(CardZoneCollectionNode),
    Targets(CardTargetOperandNode),
    LibraryTop(CardLibraryTopNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardPermissionVerb {
    Play,
    Cast,
}

/// `you may play|cast COLLECTION [DURATION]`: a player permission over cards
/// that were already bound by an earlier action. The window is part of the
/// action, either trailing or supplied by a preposed sentence duration. No
/// stated window means the effect lasts for the rest of the game (CR 611.2a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardPermissionNode {
    pub verb: CardPermissionVerb,
    pub cards: CardOperandNode,
    pub duration: Option<EffectDurationNode>,
    /// `without paying its|their mana cost(s)`: with no window, the cards are
    /// cast while the ability resolves (CR 608.2g), not later.
    pub without_paying_mana_cost: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardActionNode {
    Look {
        count: NumericExpression,
        owner: LocalPlayerReference,
    },
    Reveal(CardOperandNode),
    /// `reveal cards from the top of your library until you reveal QUANTITY
    /// PREDICATE`: every revealed card is the observed collection; the last
    /// QUANTITY matching cards are the antecedent of `that card`.
    RevealUntil {
        quantity: NumericExpression,
        predicate: ObjectPredicate,
        owner: LocalPlayerReference,
    },
    Move {
        cards: CardOperandNode,
        destination: CardMoveDestinationNode,
    },
    Permit(CardPermissionNode),
    /// `[you] choose SELECTION` (CR 608.2d): the controller chooses cards from
    /// an observed collection while the effect resolves. The choice is the
    /// singular or plural antecedent of a later `that card`, `it` or `them`.
    Choose(CardSelectionOperandNode),
    /// `shuffle [your library]` (CR 701.20a).
    Shuffle {
        owner: LocalPlayerReference,
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
    match try_recognize_card_library_position(cursor, true) {
        CollectionOperandAttempt::Recognized(fragment) => {
            return Ok(CardOperandNode::LibraryTop(fragment.node));
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
    supplied_duration: Option<&EffectDurationNode>,
) -> MachineResult<Option<CardActionNode>> {
    let Some(head) = cursor.peek() else {
        return Ok(None);
    };
    if head.eq_ignore_ascii_case("you")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("play") || raw.eq_ignore_ascii_case("cast"))
    {
        return recognize_card_permission(cursor, supplied_duration)
            .map(|node| Some(CardActionNode::Permit(node)));
    }
    if head.eq_ignore_ascii_case("choose")
        || (head.eq_ignore_ascii_case("you")
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("choose")))
    {
        // Only a selection from an observed collection belongs here; every
        // other `choose` domain stays with the choose-instruction frame.
        let mut trial = cursor.clone();
        trial.enter("card-action.choose.verb");
        if head.eq_ignore_ascii_case("you") {
            trial.consume("you")?;
        }
        trial.consume("choose")?;
        trial.enter("card-action.choose.selection");
        return match try_recognize_card_selection_operand(&mut trial) {
            CardSelectionOperandAttempt::Recognized(fragment) => {
                *cursor = trial;
                Ok(Some(CardActionNode::Choose(fragment.node)))
            }
            CardSelectionOperandAttempt::FailedCommitted(failure) => Err(failure),
            CardSelectionOperandAttempt::NotClaimed
            | CardSelectionOperandAttempt::RejectedCandidate(_) => Ok(None),
        };
    }
    if head.eq_ignore_ascii_case("shuffle") {
        cursor.enter("card-action.shuffle");
        cursor.consume("shuffle")?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
        {
            cursor.consume_phrase(&["your", "library"])?;
        }
        return Ok(Some(CardActionNode::Shuffle {
            owner: LocalPlayerReference::Controller,
        }));
    }
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
    if is_verb_form(&head, VerbLemma::Reveal, VerbForm::Base) {
        cursor.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Reveal)
                .surface(VerbForm::Base),
        )?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
        {
            return recognize_reveal_until(cursor).map(Some);
        }
        return operand(cursor).map(|value| Some(CardActionNode::Reveal(value)));
    }
    if is_verb_form(&head, VerbLemma::Exile, VerbForm::Base) {
        cursor.consume(
            crate::oracle::canonical::automata::verb_entry(VerbLemma::Exile)
                .surface(VerbForm::Base),
        )?;
        let cards = operand(cursor)?;
        let destination = recognize_card_action_destination(cursor, "exile")?;
        return Ok(Some(CardActionNode::Move { cards, destination }));
    }
    let explicit_move = is_verb_form(&head, VerbLemma::Put, VerbForm::Base)
        || is_verb_form(&head, VerbLemma::Return, VerbForm::Base);
    if !explicit_move && !move_ellipsis {
        return Ok(None);
    }
    if explicit_move {
        let verb = head.to_owned();
        cursor.consume(&verb)?;
        // `return to the battlefield all creature cards ...`: a heavy object
        // follows its destination (the same two children, reversed).
        if let Some(destination) = super::try_recognize_preposed_card_move_destination(cursor)? {
            let cards = operand(cursor)?;
            return Ok(Some(CardActionNode::Move { cards, destination }));
        }
    }
    let cards = operand(cursor)?;
    let destination = recognize_card_action_destination(cursor, "move")?;
    Ok(Some(CardActionNode::Move { cards, destination }))
}

fn recognize_reveal_until(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CardActionNode> {
    cursor.enter("card-action.reveal-until.source");
    cursor.consume_phrase(&["cards", "from", "the", "top", "of", "your", "library"])?;
    cursor.enter("card-action.reveal-until.condition");
    cursor.consume_phrase(&["until", "you", "reveal"])?;
    cursor.enter("card-action.reveal-until.quantity");
    // One object noun phrase: its count and card query; this parent
    // type-checks a chosen exact count of unscoped cards.
    let phrase = recognize_object_noun_phrase(
        cursor,
        ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Card]),
    )?;
    let quantity = match phrase.determiner {
        Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::Exactly(quantity),
            another: false,
        }) if !phrase.exclude_source => quantity,
        _ => return Err(cursor.fail("`a` or a count of revealed cards")),
    };
    if let NumericExpression::Atom(NumericAtom::Integer(value)) = &quantity {
        cursor.require(*value > 0, "a positive reveal-until quantity")?;
    }
    cursor.enter("card-action.reveal-until.predicate");
    cursor.require(
        phrase.query.relations().is_empty() && phrase.domain == Some(ObjectKind::Card),
        "an unscoped card-domain predicate for the revealed cards",
    )?;
    Ok(CardActionNode::RevealUntil {
        quantity,
        predicate: phrase.query.predicate().clone(),
        owner: LocalPlayerReference::Controller,
    })
}

fn recognize_card_permission(
    cursor: &mut OracleTokenCursor<'_>,
    supplied_duration: Option<&EffectDurationNode>,
) -> MachineResult<CardPermissionNode> {
    cursor.enter("card-action.permit.head");
    cursor.consume_phrase(&["you", "may"])?;
    let verb = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("play"))
    {
        cursor.consume("play")?;
        CardPermissionVerb::Play
    } else {
        cursor.consume("cast")?;
        CardPermissionVerb::Cast
    };
    cursor.enter("card-action.permit.cards");
    let cards = operand(cursor)?;
    cursor.require(
        matches!(
            cards,
            CardOperandNode::Reference(_) | CardOperandNode::Selection(_)
        ),
        "a previously bound card collection as the permission object",
    )?;
    cursor.enter("card-action.permit.base-cost");
    let without_paying_mana_cost = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("without"));
    if without_paying_mana_cost {
        cursor.consume_phrase(&["without", "paying"])?;
        let (possessive, noun) = if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("their"))
        {
            ("their", "costs")
        } else {
            ("its", "cost")
        };
        cursor.consume_phrase(&[possessive, "mana", noun])?;
    }
    cursor.enter("card-action.permit.window");
    let duration = match supplied_duration {
        Some(duration) => {
            cursor.require(
                !cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("until")),
                "no second duration after a sentence-level preposed duration",
            )?;
            Some(duration.clone())
        }
        None if matches!(cursor.peek(), None | Some(".")) => None,
        None => Some(recognize_play_window_duration(cursor)?),
    };
    Ok(CardPermissionNode {
        verb,
        cards,
        duration,
        without_paying_mana_cost,
    })
}

pub(in crate::oracle::canonical) fn recognize_card_action_destination(
    cursor: &mut OracleTokenCursor<'_>,
    verb: &str,
) -> MachineResult<CardMoveDestinationNode> {
    if is_verb_form(&verb, VerbLemma::Exile, VerbForm::Base) {
        return Ok(CardMoveDestinationNode {
            zone: CardZoneName::Exile,
            owner: CardMoveOwnerNode::EachCardOwner,
            position: None,
            order: None,
            tapped: false,
            entering_counters: None,
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
