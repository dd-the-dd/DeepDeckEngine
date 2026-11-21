use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, ZoneChangeUntilNode, recognize_source_reference};

/// Whether the cursor stands on `until SOURCE ...` rather than a turn-bounded
/// duration (`until end of turn`, `until your next turn`, `until the end of
/// ...`), which other readers own.
pub(in crate::oracle::canonical) fn zone_change_until_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("until"))
        && !cursor.peek_ahead(1).is_some_and(|raw| {
            ["end", "your", "the"]
                .iter()
                .any(|turn_word| raw.eq_ignore_ascii_case(turn_word))
        })
}

/// Reads the event that ends a one-shot `until` zone change (CR 610.3):
/// `until SOURCE leaves the battlefield`. The source child is the shared
/// current-object reference; the parent owns the exile it modifies.
pub(in crate::oracle::canonical) fn recognize_zone_change_until(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ZoneChangeUntilNode> {
    cursor.enter("zone-change.until.boundary");
    cursor.consume("until")?;
    cursor.enter("zone-change.until.source");
    let source = recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.require(
        source == LocalObjectReference::SourceSelf,
        "the current source object",
    )?;
    cursor.enter("zone-change.until.event");
    cursor.consume("leaves")?;
    cursor.enter("zone-change.until.zone");
    cursor.consume_phrase(&["the", "battlefield"])?;
    Ok(ZoneChangeUntilNode::SourceLeavesBattlefield)
}

#[cfg(test)]
#[path = "zone_change_until_tests.rs"]
mod tests;
