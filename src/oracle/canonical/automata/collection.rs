use super::super::{MachineResult, OracleTokenCursor};
use super::{CollectionOrder, LocalZone, NumericAtom, recognize_numeric_atom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LibraryPosition {
    Top,
    Bottom,
    /// Just beneath the top N cards (CR 401.2): `ORDINAL from the top` has
    /// ORDINAL - 1 cards above it, `just beneath the top X cards of that
    /// library` has X. A library with fewer cards puts it at the bottom.
    BeneathTopCards(NumericAtom),
}

impl LibraryPosition {
    /// The engine's `position` word and, beneath the top cards, how many
    /// stay above the placed card.
    pub(in crate::oracle::canonical) fn engine_position(
        self,
    ) -> (&'static str, Option<NumericAtom>) {
        match self {
            Self::Top => ("top", None),
            Self::Bottom => ("bottom", None),
            Self::BeneathTopCards(above) => ("top", Some(above)),
        }
    }
}

/// `ORDINAL from the top` (`second from the top`) or `just beneath the top
/// N cards of that library` after a library destination: the position below
/// the top cards. `None` consumes nothing. An ordinal word is committed: it
/// must be followed by `from the top` (`fourth` alone names no position).
pub(in crate::oracle::canonical) fn try_recognize_beneath_top_position(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<LibraryPosition>> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("just"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("beneath"))
    {
        cursor.enter("zone.library-position.beneath");
        cursor.consume_phrase(&["just", "beneath", "the", "top"])?;
        cursor.enter("zone.library-position.beneath.count");
        let above = recognize_numeric_atom(cursor)?;
        cursor.require(
            !matches!(above, NumericAtom::Integer(value) if value <= 0),
            "a positive number of cards above the placed card",
        )?;
        cursor.enter("zone.library-position.beneath.noun");
        cursor.consume(if above == NumericAtom::Integer(1) {
            "card"
        } else {
            "cards"
        })?;
        cursor.enter("zone.library-position.beneath.library");
        // The library the destination already named (CR 400.3: its owner's).
        cursor.consume_phrase(&["of", "that", "library"])?;
        return Ok(Some(LibraryPosition::BeneathTopCards(above)));
    }
    let Some(ordinal) = cursor
        .peek()
        .and_then(super::super::numeric::parse_ordinal_word)
    else {
        return Ok(None);
    };
    cursor.enter("zone.library-position.ordinal");
    let word = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&word)?;
    cursor.require(
        ordinal >= 2,
        "an ordinal of two or more (`on top of` names the top)",
    )?;
    cursor.enter("zone.library-position.ordinal.from-top");
    cursor.consume_phrase(&["from", "the", "top"])?;
    Ok(Some(LibraryPosition::BeneathTopCards(
        NumericAtom::Integer(ordinal - 1),
    )))
}

/// Recognizes a strictly positive card count while preserving `X` as a typed
/// numeric atom for the lowering layer.
pub(in crate::oracle::canonical) fn recognize_positive_card_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericAtom> {
    cursor.enter("collection.card-count");
    let count = if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("a"))
    {
        cursor.enter("collection.card-count.indefinite");
        cursor.consume("a")?;
        NumericAtom::Integer(1)
    } else {
        recognize_numeric_atom(cursor)?
    };
    cursor.require(
        !matches!(count, NumericAtom::Integer(value) if value <= 0),
        "a positive card count or X",
    )?;
    Ok(count)
}

/// Consumes the singular or plural card noun required by the recognized
/// count. Variable counts use the plural Oracle form.
pub(in crate::oracle::canonical) fn recognize_card_noun(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericAtom,
) -> MachineResult<()> {
    cursor.enter("collection.card-noun");
    match count {
        NumericAtom::Integer(1) => cursor.consume("card"),
        NumericAtom::Integer(_) | NumericAtom::VariableX => cursor.consume("cards"),
    }
}

/// Resolves an explicit controller-owned zone reference such as `your hand`
/// without reparsing a text fragment.
pub(in crate::oracle::canonical) fn recognize_controller_zone(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalZone> {
    cursor.enter("zone.possessive.controller");
    cursor.consume("your")?;
    cursor.enter("zone.possessive.kind");
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("hand") => {
            cursor.consume("hand")?;
            Ok(LocalZone::Hand)
        }
        Some("library") => {
            cursor.consume("library")?;
            Ok(LocalZone::Library)
        }
        _ => Err(cursor.fail("`hand` or `library`")),
    }
}

/// Recognizes an explicit position inside a library destination.
pub(in crate::oracle::canonical) fn recognize_library_position(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibraryPosition> {
    cursor.enter("zone.library-position.relation");
    cursor.consume("on")?;
    cursor.enter("zone.library-position.kind");
    let position = recognize_library_position_kind(cursor)?;
    cursor.enter("zone.library-position.destination");
    cursor.consume("of")?;
    Ok(position)
}

/// Recognizes how the controller orders a selected collection. The parent
/// effect decides whether chosen or random order is legal in its context.
pub(in crate::oracle::canonical) fn recognize_collection_order(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CollectionOrder> {
    cursor.enter("collection.order.relation");
    cursor.consume("in")?;
    cursor.enter("collection.order.kind");
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("any"))
    {
        cursor.consume_phrase(&["any", "order"])?;
        return Ok(CollectionOrder::Chosen);
    }
    cursor.consume_phrase(&["a", "random", "order"])?;
    Ok(CollectionOrder::Random)
}

/// Reads only the position head; relation/owner belong to the caller.
pub(in crate::oracle::canonical) fn recognize_library_position_kind(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LibraryPosition> {
    let position = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("top") => {
            cursor.consume("top")?;
            LibraryPosition::Top
        }
        Some("bottom") => {
            cursor.consume("bottom")?;
            LibraryPosition::Bottom
        }
        _ => return Err(cursor.fail("`top` or `bottom`")),
    };
    Ok(position)
}
