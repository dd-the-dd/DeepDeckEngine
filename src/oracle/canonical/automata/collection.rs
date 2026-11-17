use super::super::{MachineResult, OracleTokenCursor};
use super::{CollectionOrder, LocalZone, NumericAtom, recognize_numeric_atom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LibraryPosition {
    Top,
    Bottom,
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
