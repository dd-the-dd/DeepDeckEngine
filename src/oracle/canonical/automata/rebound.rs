//! Rebound (CR 702.88a): a keyword on instants and sorceries. Its meaning is
//! a static replacement of the resolving spell's graveyard destination and a
//! next-upkeep delayed trigger; the runtime expands the keyword into the
//! shared delayed step trigger. Reminder text has no rules meaning (CR 207.2).
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReboundKeywordNode;

/// Whether the ability is the Rebound keyword line.
pub(in crate::oracle::canonical) fn rebound_keyword_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("rebound"))
        && matches!(cursor.peek_ahead(1), None | Some("("))
}

/// Reads `Rebound` and an optional balanced reminder parenthetical.
pub(in crate::oracle::canonical) fn recognize_rebound_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ReboundKeywordNode> {
    cursor.enter("keyword.rebound.keyword");
    cursor.consume("rebound")?;
    cursor.enter("keyword.rebound.reminder-or-complete");
    if cursor.peek() == Some("(") {
        cursor.consume("(")?;
        let mut depth = 1usize;
        while depth > 0 {
            match cursor.peek() {
                None => return Err(cursor.fail("a closing parenthesis for the reminder text")),
                Some("(") => depth += 1,
                Some(")") => depth -= 1,
                Some(_) => {}
            }
            cursor.advance_to(cursor.position() + 1)?;
        }
    }
    cursor.enter("keyword.rebound.complete");
    cursor.finish()?;
    Ok(ReboundKeywordNode)
}

#[cfg(test)]
#[path = "rebound_tests.rs"]
mod tests;
