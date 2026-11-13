use super::super::{MachineResult, OracleTokenCursor};
use crate::oracle::model::RecognizedEntity;

/// One mandatory casting-cost header, independent of its payment action.
pub(in crate::oracle::canonical) const MANDATORY_ADDITIONAL_COST_HEADER: &[&str] = &[
    "as",
    "an",
    "additional",
    "cost",
    "to",
    "cast",
    "this",
    "spell",
    ",",
];

pub(in crate::oracle::canonical) fn has_mandatory_additional_cost_header(
    tokens: &[RecognizedEntity],
) -> bool {
    MANDATORY_ADDITIONAL_COST_HEADER
        .iter()
        .enumerate()
        .all(|(i, word)| {
            tokens
                .get(i)
                .is_some_and(|token| token.raw.eq_ignore_ascii_case(word))
        })
}

/// The caller retains its existing state/diagnostic name; no header is rebuilt
/// as text or tokenized a second time.
pub(in crate::oracle::canonical) fn recognize_mandatory_additional_cost_header(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    cursor.consume_phrase(MANDATORY_ADDITIONAL_COST_HEADER)
}

#[cfg(test)]
#[path = "mandatory_additional_cost_header_tests.rs"]
mod tests;
