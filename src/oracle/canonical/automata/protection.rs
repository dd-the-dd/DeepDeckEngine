//! Protection keyword atom (CR 702.16a):
//!
//! ```text
//! Protection := "protection" "from" Quality (("," | ["," ] "and") "from" Quality)*
//! Quality    := Color | "each color" | "all colors" | CardTypePlural | "everything"
//! ```
//!
//! A quality is a typed characteristic read from the shared vocabulary, never
//! a copied phrase. Only qualities the engine evaluates on a damage, blocking,
//! targeting or attachment source are admitted; any other quality (a subtype,
//! `multicolored`, `the chosen color`) is an explicit failure of this atom.
use super::super::{MachineResult, OracleTokenCursor};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, CardColor, CardType, lookup_card_color, lookup_card_type,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ProtectionQuality {
    Color(CardColor),
    /// `each color` / `all colors`: protection from every one of the five
    /// colors (CR 702.16a, 105.1).
    EachColor,
    CardType(CardType),
    Everything,
}

const COLORS: [CardColor; 5] = [
    CardColor::White,
    CardColor::Blue,
    CardColor::Black,
    CardColor::Red,
    CardColor::Green,
];

pub(in crate::oracle::canonical) fn protection_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("protection"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

pub(in crate::oracle::canonical) fn recognize_protection_quality(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ProtectionQuality> {
    cursor.enter("keyword.protection.quality");
    if word_is(cursor, 0, "everything") {
        cursor.consume("everything")?;
        return Ok(ProtectionQuality::Everything);
    }
    if word_is(cursor, 0, "each") && word_is(cursor, 1, "color") {
        cursor.consume_phrase(&["each", "color"])?;
        return Ok(ProtectionQuality::EachColor);
    }
    if word_is(cursor, 0, "all") && word_is(cursor, 1, "colors") {
        cursor.consume_phrase(&["all", "colors"])?;
        return Ok(ProtectionQuality::EachColor);
    }
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a protection quality"))?;
    if let Some(color) = lookup_card_color(raw) {
        cursor.advance_to(cursor.position() + 1)?;
        return Ok(ProtectionQuality::Color(color));
    }
    // A card-type quality is printed in its plural form (`from artifacts`).
    if let Some(card_type) = lookup_card_type(raw, AcceptedForms::SingularOrPlural)
        .filter(|card_type| raw.eq_ignore_ascii_case(card_type.oracle_plural()))
        .filter(|card_type| *card_type != CardType::Kindred)
    {
        cursor.advance_to(cursor.position() + 1)?;
        return Ok(ProtectionQuality::CardType(card_type));
    }
    Err(cursor.fail(
        "a protection quality the engine evaluates: a color, each color, a plural card type, or everything",
    ))
}

/// Reads `protection from QUALITY [and from QUALITY]…`. A list separator that
/// is not followed by `from` belongs to the enclosing keyword list.
pub(in crate::oracle::canonical) fn recognize_protection_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<ProtectionQuality>> {
    cursor.enter("keyword.protection.keyword");
    cursor.consume_phrase(&["protection", "from"])?;
    let mut qualities = vec![recognize_protection_quality(cursor)?];
    loop {
        cursor.enter("keyword.protection.separator");
        let width = if word_is(cursor, 0, "and") && word_is(cursor, 1, "from") {
            1
        } else if cursor.peek() == Some(",")
            && word_is(cursor, 1, "and")
            && word_is(cursor, 2, "from")
        {
            2
        } else if cursor.peek() == Some(",") && word_is(cursor, 1, "from") {
            1
        } else {
            break;
        };
        cursor.advance_to(cursor.position() + width)?;
        cursor.consume("from")?;
        qualities.push(recognize_protection_quality(cursor)?);
    }
    cursor.enter("keyword.protection.complete");
    cursor.require(
        qualities
            .iter()
            .enumerate()
            .all(|(index, quality)| !qualities[..index].contains(quality)),
        "distinct protection qualities",
    )?;
    Ok(qualities)
}

/// The engine's quality strings: colors and `everything` by name, card types
/// by their singular IR name. `each color` expands to the five colors.
pub(in crate::oracle::canonical) fn lower_protection_qualities(
    qualities: &[ProtectionQuality],
) -> Vec<&'static str> {
    let mut lowered = Vec::new();
    let mut push = |quality: &'static str| {
        if !lowered.contains(&quality) {
            lowered.push(quality);
        }
    };
    for quality in qualities {
        match quality {
            ProtectionQuality::Color(color) => push(color.oracle_name()),
            ProtectionQuality::EachColor => {
                COLORS.iter().for_each(|color| push(color.oracle_name()))
            }
            ProtectionQuality::CardType(card_type) => push(card_type.ir_name()),
            ProtectionQuality::Everything => push("everything"),
        }
    }
    lowered
}

#[cfg(test)]
#[path = "protection_tests.rs"]
mod tests;
