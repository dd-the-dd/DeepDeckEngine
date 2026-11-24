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

/// A permission window that the shared effect duration does not name: it
/// ends on a zone change of the permitted cards or on a later event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardPermissionWindowNode {
    /// `for as long as it|that card|they|those cards remain(s) exiled`: each
    /// permission ends when its card leaves exile (CR 400.7, 611.2b).
    WhileInExile,
    /// `until your next end step`: through the beginning of the controller's
    /// next end step.
    UntilYourNextEndStep,
    /// `until you exile another card with this NOUN`: the next card the source
    /// exiles replaces the permission (CR 607.2a linked exile).
    UntilSourceExilesAnother,
}

/// `you may play|cast CARDS [DURATION]`: a player permission over cards that
/// were already bound by an earlier action, or over a quantified card query
/// with its origin (`a spell with mana value 3 or less from your hand`).
/// The window is part of the action, either trailing or supplied by a
/// preposed sentence duration. No stated window means the effect lasts for
/// the rest of the game (CR 611.2a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardPermissionNode {
    pub verb: CardPermissionVerb,
    pub cards: CardOperandNode,
    pub duration: Option<EffectDurationNode>,
    /// A window the shared duration does not name, trailing or preposed.
    /// Never present together with `duration`.
    pub window: Option<CardPermissionWindowNode>,
    /// `without paying its|their mana cost(s)`: with no window, the cards are
    /// cast while the ability resolves (CR 608.2g), not later.
    pub without_paying_mana_cost: bool,
    /// `, and mana of any type can be spent to cast that spell|it|them`: a
    /// spending policy of the permitted casts (CR 609.4b).
    pub mana_of_any_type: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardActionNode {
    Look {
        count: NumericExpression,
        owner: LocalPlayerReference,
    },
    /// `look at OPERAND` over another player's hidden zone: the top cards of
    /// a declared player's library or a declared player's hand. Only the
    /// looking player sees the cards; they become the observed collection.
    LookAt(CardOperandNode),
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
            // `the exiled cards not cast this way`: every member of the
            // restated collection minus what a preceding cast cast.
            if matches!(
                fragment.node,
                CardCollectionReferenceNode::Antecedent
                    | CardCollectionReferenceNode::Exiled
                    | CardCollectionReferenceNode::Revealed
            ) && recognize_uncast_relative_clause(cursor)?
            {
                return Ok(CardOperandNode::Selection(CardSelectionOperandNode {
                    quantity: CardSelectionQuantityNode::All {
                        via: None,
                        except_cast: true,
                    },
                    predicate: None,
                    from: fragment.node,
                }));
            }
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
    if permission_head_starts(cursor) {
        return recognize_card_permission(cursor, supplied_duration, None)
            .map(|node| Some(CardActionNode::Permit(node)));
    }
    if head.eq_ignore_ascii_case("for") || head.eq_ignore_ascii_case("until") {
        // `For as long as that card remains exiled, you may play it.` /
        // `Until your next end step, you may play those cards.`: a window
        // this child claims only when a complete, comma-closed window is
        // followed by a permission head; every other preposed phrase stays
        // with the sequence's own duration introducer.
        let Some((mut trial, window)) = try_preposed_permission_window(cursor) else {
            return Ok(None);
        };
        cursor.require(
            supplied_duration.is_none(),
            "one preposed window for one permission",
        )?;
        let node = recognize_card_permission(&mut trial, None, Some(window))?;
        *cursor = trial;
        return Ok(Some(CardActionNode::Permit(node)));
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
        if super::shuffle_object_follows(cursor) {
            // `shuffle OBJECT into LIBRARY` (CR 701.24b): the shared card
            // operand, then the library destination that owns the shuffle.
            cursor.enter("card-action.shuffle.object");
            let cards = operand(cursor)?;
            let destination = super::recognize_shuffle_library_destination(cursor)?;
            return Ok(Some(CardActionNode::Move { cards, destination }));
        }
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
        return recognize_look(cursor).map(Some);
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

/// `look at OPERAND`: the shared library-position operand (the controller's
/// or a declared player's library) or the shared zone collection naming a
/// declared player's whole hand. The controller's own library keeps the
/// historical `Look` node; every other owner is a private `LookAt`.
fn recognize_look(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CardActionNode> {
    cursor.enter("card-action.look.source");
    cursor.consume_phrase(&["look", "at"])?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
    {
        cursor.enter("card-action.look.library");
        let top = match try_recognize_card_library_position(cursor, true) {
            CollectionOperandAttempt::Recognized(fragment) => fragment.node,
            CollectionOperandAttempt::NotClaimed => {
                return Err(cursor.fail("the top cards of a library"));
            }
            CollectionOperandAttempt::RejectedCandidate(failure)
            | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        };
        return Ok(match top.owner {
            CardLibraryOwnerNode::Player(LocalPlayerReference::Controller) => {
                CardActionNode::Look {
                    count: top.count,
                    owner: LocalPlayerReference::Controller,
                }
            }
            _ => CardActionNode::LookAt(CardOperandNode::LibraryTop(top)),
        });
    }
    cursor.enter("card-action.look.zone");
    let zone = match try_recognize_card_zone_collection(cursor) {
        CollectionOperandAttempt::Recognized(fragment) => fragment.node,
        CollectionOperandAttempt::NotClaimed => {
            return Err(cursor.fail("the top cards of a library or a targeted player's hand"));
        }
        CollectionOperandAttempt::RejectedCandidate(failure)
        | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
    };
    cursor.require(
        zone.zone == CardZoneName::Hand
            && zone.predicate.is_none()
            && zone.quantity.is_none()
            && matches!(zone.owner, CardZoneOwnerNode::TargetPlayer(_)),
        "a targeted player's whole hand (a filtered look reveals only part of it)",
    )?;
    Ok(CardActionNode::LookAt(CardOperandNode::ZoneCollection(
        zone,
    )))
}

fn peek_word(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// `you may play|cast`: the head of a card permission. Pure lookahead.
fn permission_head_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_word(cursor, 0, "you")
        && peek_word(cursor, 1, "may")
        && (peek_word(cursor, 2, "play") || peek_word(cursor, 2, "cast"))
}

/// A preposed `WINDOW,` followed by a permission head, on a trial cursor.
fn try_preposed_permission_window<'a>(
    cursor: &OracleTokenCursor<'a>,
) -> Option<(OracleTokenCursor<'a>, CardPermissionWindowNode)> {
    let mut trial = cursor.clone();
    let window = if peek_word(&trial, 0, "for") {
        recognize_exiled_window(&mut trial).ok()?
    } else if peek_word(&trial, 1, "your") {
        recognize_next_end_step_window(&mut trial).ok()?
    } else {
        return None;
    };
    trial.consume(",").ok()?;
    permission_head_starts(&trial).then_some((trial, window))
}

/// `for as long as it|they|that card|those cards|the exiled card(s)
/// remain(s) exiled`. The subject restates the permitted cards; the window
/// applies to each permitted card separately.
fn recognize_exiled_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardPermissionWindowNode> {
    cursor.enter("card-action.permit.window.while-exiled");
    cursor.consume_phrase(&["for", "as", "long", "as"])?;
    cursor.enter("card-action.permit.window.while-exiled.subject");
    let plural = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("it") => {
            cursor.consume("it")?;
            false
        }
        Some("they") => {
            cursor.consume("they")?;
            true
        }
        Some("that") => {
            cursor.consume_phrase(&["that", "card"])?;
            false
        }
        Some("those") => {
            cursor.consume_phrase(&["those", "cards"])?;
            true
        }
        Some("the") => {
            cursor.consume_phrase(&["the", "exiled"])?;
            let plural = peek_word(cursor, 0, "cards");
            cursor.consume(if plural { "cards" } else { "card" })?;
            plural
        }
        _ => {
            return Err(cursor.fail("the permitted cards: it, they, that card or those cards"));
        }
    };
    cursor.enter("card-action.permit.window.while-exiled.verb");
    cursor.consume(if plural { "remain" } else { "remains" })?;
    cursor.consume("exiled")?;
    Ok(CardPermissionWindowNode::WhileInExile)
}

/// `until your next end step`.
fn recognize_next_end_step_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardPermissionWindowNode> {
    cursor.enter("card-action.permit.window.next-end-step");
    cursor.consume_phrase(&["until", "your", "next", "end", "step"])?;
    Ok(CardPermissionWindowNode::UntilYourNextEndStep)
}

/// `until you exile another card with this NOUN`: the source noun is a
/// registered object or subtype word, as in `cards exiled with this NOUN`.
fn recognize_source_exile_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardPermissionWindowNode> {
    cursor.enter("card-action.permit.window.source-exiles-another");
    cursor.consume_phrase(&["until", "you", "exile", "another", "card", "with", "this"])?;
    let noun = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a source noun after `with this`"))?
        .to_string();
    cursor.require(
        crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(&noun).is_some()
            || crate::oracle::characteristic_vocabulary::lookup_noncreature_subtype(&noun)
                .is_some(),
        "a registered source noun after `with this`",
    )?;
    cursor.consume(&noun)?;
    Ok(CardPermissionWindowNode::UntilSourceExilesAnother)
}

/// Whether a trailing permission window starts at the cursor.
fn permission_window_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    peek_word(cursor, 0, "until")
        || (peek_word(cursor, 0, "this") && peek_word(cursor, 1, "turn"))
        || (peek_word(cursor, 0, "for") && peek_word(cursor, 1, "as"))
}

/// The trailing window of a permission: the shared play-window duration, the
/// source-control duration, or a window of this family.
fn recognize_trailing_permission_window(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(Option<EffectDurationNode>, Option<CardPermissionWindowNode>)> {
    if peek_word(cursor, 0, "for") {
        if peek_word(cursor, 4, "you") && peek_word(cursor, 5, "control") {
            // `for as long as you control this creature` (CR 611.2b). A
            // printed name needs the sequence's name links, which this child
            // does not receive: it stays an explicit failure.
            return Ok((
                Some(recognize_source_control_duration(cursor, &[], None)?),
                None,
            ));
        }
        return Ok((None, Some(recognize_exiled_window(cursor)?)));
    }
    if peek_word(cursor, 0, "until") && peek_word(cursor, 1, "your") && peek_word(cursor, 3, "end")
    {
        return Ok((None, Some(recognize_next_end_step_window(cursor)?)));
    }
    if peek_word(cursor, 0, "until") && peek_word(cursor, 1, "you") {
        return Ok((None, Some(recognize_source_exile_window(cursor)?)));
    }
    Ok((Some(recognize_play_window_duration(cursor)?), None))
}

/// `without paying its|their mana cost(s)`.
fn recognize_without_paying(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    if !peek_word(cursor, 0, "without") {
        return Ok(false);
    }
    cursor.enter("card-action.permit.base-cost");
    cursor.consume_phrase(&["without", "paying"])?;
    let (possessive, noun) = if peek_word(cursor, 0, "their") {
        ("their", "costs")
    } else {
        ("its", "cost")
    };
    cursor.consume_phrase(&[possessive, "mana", noun])?;
    Ok(true)
}

/// `[,] and mana of any type can be spent to cast that spell|it|them|those
/// spells`: the permitted casts' spending policy, coordinated with the
/// permission it qualifies.
fn any_type_spending_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let offset = usize::from(peek_word(cursor, 0, ","));
    peek_word(cursor, offset, "and") && peek_word(cursor, offset + 1, "mana")
}

fn recognize_any_type_spending(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    if !any_type_spending_starts(cursor) {
        return Ok(false);
    }
    let comma = peek_word(cursor, 0, ",");
    cursor.enter("card-action.permit.spending.any-type");
    if comma {
        cursor.consume(",")?;
    }
    cursor.consume_phrase(&[
        "and", "mana", "of", "any", "type", "can", "be", "spent", "to", "cast",
    ])?;
    cursor.enter("card-action.permit.spending.spells");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("it") => cursor.consume("it")?,
        Some("them") => cursor.consume("them")?,
        Some("that") => cursor.consume_phrase(&["that", "spell"])?,
        Some("those") => cursor.consume_phrase(&["those", "spells"])?,
        _ => return Err(cursor.fail("the permitted spells: that spell, it, them or those spells")),
    }
    Ok(true)
}

/// Where the permitted cards of a quantified query are.
enum PermittedCardOrigin {
    /// `from among them|those cards|the exiled cards|cards exiled with this`.
    Collection(CardCollectionReferenceNode),
    /// `from your|target player's ZONE`.
    Zone {
        zone: CardZoneName,
        owner: CardZoneOwnerNode,
    },
}

fn recognize_permitted_card_origin(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PermittedCardOrigin> {
    if peek_word(cursor, 0, "among") {
        cursor.enter("card-action.permit.origin.collection");
        cursor.consume("among")?;
        let reference = match try_recognize_card_collection_reference(cursor) {
            CollectionOperandAttempt::Recognized(fragment) => fragment.node,
            CollectionOperandAttempt::NotClaimed => {
                return Err(cursor.fail("a bound card collection after `from among`"));
            }
            CollectionOperandAttempt::RejectedCandidate(failure)
            | CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        };
        cursor.require(
            matches!(
                reference,
                CardCollectionReferenceNode::Antecedent
                    | CardCollectionReferenceNode::Exiled
                    | CardCollectionReferenceNode::Linked
            ),
            "them, those cards, the exiled cards or cards exiled with the source",
        )?;
        return Ok(PermittedCardOrigin::Collection(reference));
    }
    cursor.enter("card-action.permit.origin.zone");
    let owner = if peek_word(cursor, 0, "your") {
        cursor.consume("your")?;
        CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
    } else if starts_possessive_target_player(cursor) {
        CardZoneOwnerNode::TargetPlayer(recognize_possessive_target_player(cursor)?)
    } else {
        return Err(
            cursor.fail("`among` a bound collection, `your` zone or a targeted player's zone")
        );
    };
    let zone = recognize_card_zone_atom(cursor)?;
    cursor.require(
        matches!(
            zone,
            CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
        ),
        "a hand, graveyard or exile origin of the permitted cards",
    )?;
    Ok(PermittedCardOrigin::Zone { zone, owner })
}

/// CR 601.2a: a card becomes a spell with the same characteristics, so a
/// spell query over the cards to cast reads as the same query in the card
/// domain. Only the domain noun changes; every characteristic is kept.
fn spell_query_as_card_query(predicate: &ObjectPredicate) -> ObjectPredicate {
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Spell) => {
            ObjectPredicate::ObjectKind(ObjectKind::Card)
        }
        ObjectPredicate::Not(operand) => {
            ObjectPredicate::Not(Box::new(spell_query_as_card_query(operand)))
        }
        ObjectPredicate::Intersection(operands) => {
            ObjectPredicate::Intersection(operands.iter().map(spell_query_as_card_query).collect())
        }
        ObjectPredicate::Union(operands) => {
            ObjectPredicate::Union(operands.iter().map(spell_query_as_card_query).collect())
        }
        other => other.clone(),
    }
}

/// `... from your graveyard with mana value 4 or less`: a numeric qualifier
/// placed after the origin still qualifies the whole query.
fn recognize_postposed_numeric_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
    predicate: ObjectPredicate,
) -> MachineResult<ObjectPredicate> {
    let property_follows = peek_word(cursor, 1, "mana")
        || peek_word(cursor, 1, "power")
        || peek_word(cursor, 1, "toughness");
    if !(peek_word(cursor, 0, "with") && property_follows) {
        return Ok(predicate);
    }
    cursor.enter("card-action.permit.query.postposed-qualifier");
    cursor.consume("with")?;
    let property = recognize_selector_numeric_property(cursor)?;
    let comparison = recognize_numeric_property_comparison_tail(cursor, property)?;
    Ok(match predicate {
        ObjectPredicate::Intersection(mut operands) => {
            operands.push(comparison);
            ObjectPredicate::Intersection(operands)
        }
        other => ObjectPredicate::Intersection(vec![other, comparison]),
    })
}

/// `a|N|up to N|any number of|[bare plural] SPELL-OR-CARD-QUERY from ORIGIN`:
/// the permitted cards of a permission as one shared noun phrase (its
/// quantity and query) and an origin. A spell query (`cast`) is read in the
/// card domain. Targets, references and library positions stay with the
/// shared card operand; the phrase is committed at its `from`.
fn try_recognize_permitted_card_query(
    cursor: &mut OracleTokenCursor<'_>,
    verb: CardPermissionVerb,
) -> CollectionOperandAttempt<CardOperandNode> {
    let mut trial = cursor.clone();
    let mut committed = false;
    let result = (|| -> MachineResult<CardOperandNode> {
        trial.enter("card-action.permit.query");
        let phrase = recognize_object_noun_phrase(
            &mut trial,
            ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Spell, ObjectKind::Card]),
        )?;
        let quantity = match phrase.determiner {
            None | Some(ObjectDeterminer::Universal { each: false }) => {
                CardSelectionQuantityNode::All {
                    via: None,
                    except_cast: false,
                }
            }
            Some(ObjectDeterminer::Chosen {
                quantity,
                another: false,
            }) => match quantity {
                ChosenObjectQuantity::Exactly(count) => CardSelectionQuantityNode::Exactly(count),
                ChosenObjectQuantity::UpTo(count) => CardSelectionQuantityNode::UpTo(count),
                ChosenObjectQuantity::AnyNumber => CardSelectionQuantityNode::AnyNumber,
            },
            _ => return Err(trial.fail("a selection quantity or a bare plural of permitted cards")),
        };
        trial.require(
            !phrase.exclude_source && phrase.query.relations().is_empty(),
            "an unscoped spell or card query without a source exclusion",
        )?;
        let predicate = match phrase.domain {
            Some(ObjectKind::Spell) => {
                trial.require(
                    verb == CardPermissionVerb::Cast,
                    "`cast` for a permitted spell query",
                )?;
                spell_query_as_card_query(phrase.query.predicate())
            }
            Some(ObjectKind::Card) => phrase.query.predicate().clone(),
            _ => return Err(trial.fail("a spell or card query")),
        };
        trial.enter("card-action.permit.query.origin");
        trial.consume("from")?;
        committed = true;
        let origin = recognize_permitted_card_origin(&mut trial)?;
        let predicate = recognize_postposed_numeric_qualifier(&mut trial, predicate)?;
        trial.enter("card-action.permit.query.domain");
        trial.require(
            predicate.is_in_domain(ObjectKind::Card),
            "a card-domain query over the permitted cards",
        )?;
        let predicate =
            (predicate != ObjectPredicate::ObjectKind(ObjectKind::Card)).then_some(predicate);
        Ok(match origin {
            PermittedCardOrigin::Collection(from) => {
                CardOperandNode::Selection(CardSelectionOperandNode {
                    quantity,
                    predicate,
                    from,
                })
            }
            PermittedCardOrigin::Zone { zone, owner } => {
                CardOperandNode::ZoneCollection(CardZoneCollectionNode {
                    zone,
                    owner,
                    predicate,
                    quantity: match quantity {
                        CardSelectionQuantityNode::All { .. } => None,
                        other => Some(other),
                    },
                })
            }
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("card-action.permit.query.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(failure) if committed => CollectionOperandAttempt::FailedCommitted(failure),
        Err(failure) => CollectionOperandAttempt::RejectedCandidate(failure),
    }
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
    preposed_window: Option<CardPermissionWindowNode>,
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
    let cards = match try_recognize_permitted_card_query(cursor, verb) {
        CollectionOperandAttempt::Recognized(fragment) => fragment.node,
        CollectionOperandAttempt::FailedCommitted(failure) => return Err(failure),
        CollectionOperandAttempt::NotClaimed | CollectionOperandAttempt::RejectedCandidate(_) => {
            operand(cursor)?
        }
    };
    cursor.enter("card-action.permit.object");
    cursor.require(
        matches!(
            cards,
            CardOperandNode::Reference(_)
                | CardOperandNode::Selection(_)
                | CardOperandNode::ZoneCollection(_)
                | CardOperandNode::Targets(_)
        ),
        "a bound card collection, declared card targets or a quantified card query as the permission object",
    )?;
    let mut without_paying_mana_cost = recognize_without_paying(cursor)?;
    cursor.enter("card-action.permit.window");
    let (duration, window) = if supplied_duration.is_some() || preposed_window.is_some() {
        cursor.require(
            !permission_window_starts(cursor),
            "no second duration after a sentence-level preposed duration",
        )?;
        (supplied_duration.cloned(), preposed_window)
    } else if permission_window_starts(cursor) {
        recognize_trailing_permission_window(cursor)?
    } else if matches!(cursor.peek(), None | Some(".")) || any_type_spending_starts(cursor) {
        (None, None)
    } else {
        (Some(recognize_play_window_duration(cursor)?), None)
    };
    if !without_paying_mana_cost {
        // `this turn without paying their mana costs`: the base-cost policy
        // may follow the window.
        without_paying_mana_cost = recognize_without_paying(cursor)?;
    }
    cursor.enter("card-action.permit.spending");
    let mana_of_any_type = recognize_any_type_spending(cursor)?;
    Ok(CardPermissionNode {
        verb,
        cards,
        duration,
        window,
        without_paying_mana_cost,
        mana_of_any_type,
    })
}

pub(in crate::oracle::canonical) fn recognize_card_action_destination(
    cursor: &mut OracleTokenCursor<'_>,
    verb: &str,
) -> MachineResult<CardMoveDestinationNode> {
    if is_verb_form(&verb, VerbLemma::Exile, VerbForm::Base) {
        // `exile OBJECT face down`: the shared entry-modifier list of the
        // exile destination (CR 406.3).
        let mut modifiers = EntryModifiers::new(CardMoveOwnerNode::EachCardOwner);
        recognize_entry_modifiers(cursor, CardZoneName::Exile, &mut modifiers)?;
        return Ok(CardMoveDestinationNode {
            zone: CardZoneName::Exile,
            owner: CardMoveOwnerNode::EachCardOwner,
            position: None,
            order: None,
            tapped: false,
            entering_counters: None,
            face_down: modifiers.face_down,
            transformed: false,
            shuffle: false,
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
