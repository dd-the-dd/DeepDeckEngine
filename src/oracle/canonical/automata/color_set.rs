use crate::oracle::characteristic_vocabulary::lookup_card_color;

use super::super::{MachineResult, OracleTokenCursor};
use super::CardColor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardColorSetNode {
    pub colors: Vec<CardColor>,
}

pub(in crate::oracle::canonical) fn recognize_card_color_set(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CardColorSetNode> {
    let mut colors = Vec::new();
    loop {
        cursor.enter("characteristic.color-set.item");
        let raw = cursor
            .peek()
            .ok_or_else(|| cursor.fail("a Magic color word"))?;
        let color = lookup_card_color(raw).ok_or_else(|| cursor.fail("a Magic color word"))?;
        cursor.require(!colors.contains(&color), "distinct Magic colors")?;
        colors.push(color);
        cursor.advance_to(cursor.position() + 1)?;

        cursor.enter("characteristic.color-set.separator-or-complete");
        // A comma belongs to the list only when another color follows it
        // (`white, blue, and black`); otherwise it is the parent's boundary.
        let comma_continues = cursor.peek().is_some_and(|raw| raw == ",")
            && (cursor.peek_ahead(1).and_then(lookup_card_color).is_some()
                || (cursor
                    .peek_ahead(1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                    && cursor.peek_ahead(2).and_then(lookup_card_color).is_some()));
        if comma_continues {
            cursor.consume(",")?;
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            {
                cursor.consume("and")?;
            }
            continue;
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && cursor.peek_ahead(1).and_then(lookup_card_color).is_some()
        {
            cursor.consume("and")?;
            continue;
        }
        cursor.enter("characteristic.color-set.complete");
        return Ok(CardColorSetNode { colors });
    }
}

/// How many colors a resolution-time color choice names (CR 105.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChosenColorCount {
    /// `the color of your choice`: exactly one color.
    One,
    /// `the color or colors of your choice`: one or more colors.
    OneOrMore,
}

/// The color complement of a characteristic-setting verb (`becomes white`,
/// `becomes colorless`, `becomes all colors`, `becomes the color of your
/// choice`). Every form sets the object's color in layer 5 (CR 105.3,
/// 613.1e); the parent owns the subject and the duration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ColorComplementNode {
    /// A stated set of colors; `colorless` is the empty set (CR 105.2c).
    Colors(CardColorSetNode),
    /// `all colors`: each of the five colors (CR 105.1).
    AllColors,
    /// A choice made by the ability's controller as the effect resolves.
    Chosen(ChosenColorCount),
}

/// True when a color complement starts at the cursor.
pub(in crate::oracle::canonical) fn color_complement_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    cursor.peek().and_then(lookup_card_color).is_some()
        || word(0, "colorless")
        || (word(0, "all") && word(1, "colors"))
        || (word(0, "the") && word(1, "color"))
}

/// `COLORS` | `colorless` | `all colors` | `the color [or colors] of your
/// choice`. A color list keeps the shared list reader, so its connector
/// boundary is the same as in every other color consumer.
pub(in crate::oracle::canonical) fn recognize_color_complement(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ColorComplementNode> {
    cursor.enter("characteristic.color-complement");
    let first = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a color complement"))?;
    match first.as_str() {
        "colorless" => {
            cursor.consume("colorless")?;
            Ok(ColorComplementNode::Colors(CardColorSetNode {
                colors: Vec::new(),
            }))
        }
        "all" => {
            cursor.consume_phrase(&["all", "colors"])?;
            Ok(ColorComplementNode::AllColors)
        }
        "the" => {
            cursor.enter("characteristic.color-complement.choice");
            cursor.consume_phrase(&["the", "color"])?;
            let count = if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
            {
                cursor.consume_phrase(&["or", "colors"])?;
                ChosenColorCount::OneOrMore
            } else {
                ChosenColorCount::One
            };
            cursor.consume_phrase(&["of", "your", "choice"])?;
            Ok(ColorComplementNode::Chosen(count))
        }
        _ => recognize_card_color_set(cursor).map(ColorComplementNode::Colors),
    }
}

#[cfg(test)]
#[path = "color_set_tests.rs"]
mod tests;
