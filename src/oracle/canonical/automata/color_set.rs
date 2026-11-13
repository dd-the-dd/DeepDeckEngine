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
        if cursor.peek().is_some_and(|raw| raw == ",") {
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

#[cfg(test)]
#[path = "color_set_tests.rs"]
mod tests;
