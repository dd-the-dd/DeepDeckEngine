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
    /// `each player's library`, `each opponent's library`: the library of
    /// every player of a population, read on resolution (CR 401.1).
    Players(PlayerSetReferenceNode),
}

/// `each player's` / `each opponent's` before a library: the population
/// whose libraries the operand reads. `None` consumes nothing.
fn recognize_library_owner_population(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<PlayerSetReferenceNode>> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
    {
        return Ok(None);
    }
    let role = cursor
        .peek_ahead(1)
        .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase());
    let players = match role.as_deref() {
        Some("player's") => PlayerSetReferenceNode::EachPlayer,
        Some("opponent's") => PlayerSetReferenceNode::OpponentsOfAbilityController,
        _ => return Ok(None),
    };
    cursor.enter("collection.library-top.population-owner");
    cursor.advance_to(cursor.position() + 2)?;
    Ok(Some(players))
}

/// `the top N cards` with no written library: the library the same sentence
/// named last before it (`Shuffle your library, then exile the top four
/// cards.`). Only the controller's own library is an antecedent here; any
/// other or no earlier library in the sentence fails explicitly.
fn recognize_elided_library_owner(
    cursor: &OracleTokenCursor<'_>,
) -> MachineResult<CardLibraryOwnerNode> {
    let earlier = cursor.token_slice(0, cursor.position());
    let sentence = &earlier[earlier
        .iter()
        .rposition(|token| token.raw == ".")
        .map_or(0, |end| end + 1)..];
    let named = sentence
        .windows(2)
        .rev()
        .find(|pair| pair[1].raw.eq_ignore_ascii_case("library"))
        .map(|pair| pair[0].raw.eq_ignore_ascii_case("your"));
    cursor.require(
        named == Some(true),
        "`your library` named earlier in the sentence for an elided library",
    )?;
    Ok(CardLibraryOwnerNode::Player(
        LocalPlayerReference::Controller,
    ))
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

/// `COUNT cards from the top of`: the counted form of the same position
/// (`exile that many cards from the top of your library`, CR 401.2). A pure
/// lookahead over the amount and the five position words.
fn counted_from_top_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
    {
        return false;
    }
    let mut trial = cursor.clone();
    if recognize_numeric_expression(&mut trial, None).is_err() {
        return false;
    }
    ["cards", "from", "the", "top", "of"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            trial
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// The end of an ordered zone a position names (CR 401.2 for a library;
/// a graveyard keeps the order its cards were put into it, the most recent
/// on top).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardZoneEnd {
    Top,
    Bottom,
}

impl CardZoneEnd {
    /// The runtime end name of `graveyardEndCards`.
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// `the top|bottom [N] [QUERY] card(s) of OWNER graveyard`: the N matching
/// cards nearest that end of one player's graveyard, counted among the
/// matching cards only (`the top creature card of your graveyard` is the
/// most recently put creature card). Every card-action verb consumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardGraveyardPositionNode {
    pub end: CardZoneEnd,
    pub count: NumericExpression,
    /// The card query; `None` for the bare `card` noun.
    pub predicate: Option<ObjectPredicate>,
    pub owner: CardZoneOwnerNode,
}

/// The offset of the `of` that closes `the top|bottom ...` when the zone it
/// introduces is a graveyard (`of your graveyard`, `of target player's
/// graveyard`, `of defending player's graveyard`). Pure lookahead within
/// the clause; a library position (`the top card of your library`) is
/// `None` and stays with the library reader.
fn graveyard_position_of_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !(word(0, "the") && (word(1, "top") || word(1, "bottom"))) {
        return None;
    }
    let of = (2..12)
        .take_while(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| word(*offset, "of"))?;
    // The owner phrase is one to three words (`your`, `target player's`,
    // `defending player's`) before the zone noun.
    (of + 2..of + 5)
        .any(|offset| word(offset, "graveyard"))
        .then_some(of)
}

/// The graveyard position operand (see [`CardGraveyardPositionNode`]). It
/// is committed once the lookahead proved `the top|bottom ... of ...
/// graveyard`: every later failure (an unread query, an unsupported owner)
/// is this operand's own diagnostic.
pub(in crate::oracle::canonical) fn try_recognize_card_graveyard_position(
    cursor: &mut OracleTokenCursor<'_>,
) -> CollectionOperandAttempt<CardGraveyardPositionNode> {
    let Some(of_offset) = graveyard_position_of_offset(cursor) else {
        return CollectionOperandAttempt::NotClaimed;
    };
    let of = cursor.position() + of_offset;
    let mut trial = cursor.clone();
    let result = (|| -> MachineResult<CardGraveyardPositionNode> {
        trial.enter("collection.graveyard-position.end");
        trial.consume("the")?;
        let end = if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("top"))
        {
            trial.consume("top")?;
            CardZoneEnd::Top
        } else {
            trial.consume("bottom")?;
            CardZoneEnd::Bottom
        };
        trial.enter("collection.graveyard-position.quantity");
        // `the top three black cards`: a written count before the query;
        // without one the position names one card.
        let mut counted = trial.clone();
        let count = match recognize_numeric_expression(&mut counted, None) {
            Ok(count) if counted.position() < of => {
                trial = counted;
                Some(count)
            }
            _ => None,
        };
        trial.enter("collection.graveyard-position.cards");
        let query = trial.bounded(of, |inner| {
            recognize_object_noun_query(inner, ObjectQueryContext::ORDINARY)
        })?;
        let plural = trial
            .token_slice(of - 1, of)
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("cards"));
        let singular = trial
            .token_slice(of - 1, of)
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("card"));
        trial.require(
            (plural || singular)
                && query.relations().is_empty()
                && query.predicate().is_in_domain(ObjectKind::Card),
            "an unscoped card query ending with `card` or `cards`",
        )?;
        let count = count.unwrap_or_else(|| NumericExpression::from(1));
        if let NumericExpression::Atom(NumericAtom::Integer(value)) = &count {
            trial.require(*value > 0, "a positive number of positioned cards")?;
            trial.require(
                (*value == 1) == singular,
                "`card` for one positioned card, `cards` for several",
            )?;
        } else {
            trial.require(plural, "`cards` after a variable count")?;
        }
        let predicate = Some(query.predicate().clone())
            .filter(|predicate| *predicate != ObjectPredicate::ObjectKind(ObjectKind::Card));
        trial.enter("collection.graveyard-position.owner");
        trial.consume("of")?;
        let owner = if trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
        {
            trial.consume("your")?;
            CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
        } else if starts_possessive_target_player(&trial) {
            CardZoneOwnerNode::TargetPlayer(recognize_possessive_target_player(&mut trial)?)
        } else if let Some(player) = try_recognize_bound_possessive_player(&mut trial)? {
            CardZoneOwnerNode::Player(LocalPlayerReference::Bound(player))
        } else {
            return Err(trial.fail(
                "`your`, a target player's or a bound player's graveyard after the position",
            ));
        };
        trial.enter("collection.graveyard-position.zone");
        let zone = recognize_card_zone_atom(&mut trial)?;
        trial.require(
            zone == CardZoneName::Graveyard,
            "the graveyard whose order the position reads",
        )?;
        Ok(CardGraveyardPositionNode {
            end,
            count,
            predicate,
            owner,
        })
    })();
    match result {
        Ok(node) => {
            trial.enter("collection.graveyard-position.complete");
            let end_position = trial.position();
            *cursor = trial;
            CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
        }
        Err(failure) => CollectionOperandAttempt::FailedCommitted(failure),
    }
}

/// The library position operand of an action verb: the controller's
/// library or, when `target_owner` is admitted, a declared player's
/// (`the top N cards of target opponent's library`). Another zone after
/// `of` is not this operand and stays an explicit failure of the action.
/// `N cards from the top of LIBRARY` names the same cards.
pub(in crate::oracle::canonical) fn try_recognize_card_library_position(
    cursor: &mut OracleTokenCursor<'_>,
    target_owner: bool,
) -> CollectionOperandAttempt<CardLibraryTopNode> {
    let the_top = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("top"));
    if !the_top && !counted_from_top_starts(cursor) {
        return CollectionOperandAttempt::NotClaimed;
    }
    let mut trial = cursor.clone();
    let mut committed = false;
    let result = (|| -> MachineResult<CardLibraryTopNode> {
        let count = if the_top {
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
            count
        } else {
            // The lookahead proved `cards from the top of` after the amount.
            trial.enter("collection.library-top.counted-quantity");
            let count = recognize_numeric_expression(&mut trial, None)?;
            committed = true;
            if let NumericExpression::Atom(NumericAtom::Integer(value)) = &count {
                trial.require(
                    *value > 1,
                    "a library-top quantity above one before `cards`",
                )?;
            }
            trial.enter("collection.library-top.counted-position");
            trial.consume_phrase(&["cards", "from", "the", "top"])?;
            count
        };
        trial.enter("collection.library-top.owner");
        if the_top
            && !trial
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
        {
            // `Shuffle your library, then exile the top card.`: the library
            // is elided after the one the same sentence named.
            return recognize_elided_library_owner(&trial)
                .map(|owner| CardLibraryTopNode { count, owner });
        }
        trial.consume("of")?;
        let owner = if target_owner && starts_possessive_target_player(&trial) {
            // The possessive target head is the shared target-list form:
            // the declared player owns the library (CR 115.1, 401.1).
            CardLibraryOwnerNode::TargetPlayer(recognize_possessive_target_player(&mut trial)?)
        } else if target_owner
            && let Some(player) = try_recognize_bound_possessive_player(&mut trial)?
        {
            // `that player's library` / `defending player's library`: the
            // player the binding environment names (the event's player, an
            // earlier declared player) owns the library (CR 401.1). The
            // shared bound-player resolver reads the possessive head. Like a
            // declared owner, only a consumer that lowers the owner admits it.
            trial.enter("collection.library-top.bound-owner");
            CardLibraryOwnerNode::Player(LocalPlayerReference::Bound(player))
        } else if target_owner
            && let Some(players) = recognize_library_owner_population(&mut trial)?
        {
            // `the top card of each player's library`: one library per
            // player of the population; only a consumer that lowers the
            // owner admits it, like a declared owner.
            CardLibraryOwnerNode::Players(players)
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
