//! The repetition adjunct of an action: `ACTION twice`, `ACTION N times`.
//!
//! ```text
//! Repetition := "twice" | "thrice" | Amount "times"
//! Amount     := the shared numeric expression, bounded before `times`
//! ```
//!
//! The adjunct says the whole instruction is performed that many times in a
//! row, each performance a separate event (two investigations create two
//! Clues one at a time). The amount is read once, before the first
//! performance. The reader is shared; an action owns its repetition only
//! when its node carries the count, and the sequence refuses the adjunct
//! after any other action instead of reading it as a multiplied quantity.
use super::super::{MachineResult, OracleTokenCursor};
use super::{NumericExpression, recognize_numeric_expression};

/// The longest amount before `times` this adjunct reads (`X`, `three`,
/// `that many`).
const MAXIMUM_AMOUNT_TOKENS: usize = 3;

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The offset of `times` closing an amount at the cursor, inside the
/// current clause.
fn times_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (1..=MAXIMUM_AMOUNT_TOKENS)
        .take_while(|offset| {
            cursor.peek_ahead(offset - 1).is_some_and(|raw| {
                !matches!(raw, "." | "," | ";" | ":" | "(")
                    && !["and", "then", "or"]
                        .iter()
                        .any(|connector| raw.eq_ignore_ascii_case(connector))
            })
        })
        .find(|offset| word_at(cursor, *offset, "times"))
}

/// True when a repetition adjunct starts at the cursor.
pub(in crate::oracle::canonical) fn repetition_adjunct_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "twice") || word_at(cursor, 0, "thrice") || times_offset(cursor).is_some()
}

/// Reads one repetition adjunct and returns how many times the action is
/// performed.
pub(in crate::oracle::canonical) fn recognize_repetition_adjunct(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("action.repetition");
    for (word, count) in [("twice", 2_i64), ("thrice", 3_i64)] {
        if word_at(cursor, 0, word) {
            cursor.consume(word)?;
            return Ok(NumericExpression::from(count));
        }
    }
    let offset = times_offset(cursor)
        .ok_or_else(|| cursor.fail("`twice`, `thrice` or an amount followed by `times`"))?;
    cursor.enter("action.repetition.amount");
    // `times` is also the multiplication operator of the shared arithmetic:
    // the amount is read in a view that ends before it.
    let end = cursor.position() + offset;
    let count = cursor.bounded(end, |amount| recognize_numeric_expression(amount, None))?;
    cursor.enter("action.repetition.times");
    cursor.consume("times")?;
    Ok(count)
}

#[cfg(test)]
#[path = "repetition_tests.rs"]
mod tests;
