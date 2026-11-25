use crate::oracle::counter_name::CounterName;

use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterNameState {
    FirstPart,
    MoreParts,
    Complete,
}

/// Consumes an opaque, contextually resolved counter name and leaves the
/// following `counter(s)` noun for the owning effect grammar.
pub(in crate::oracle::canonical) fn recognize_counter_name(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterName> {
    let mut state = CounterNameState::FirstPart;
    let mut parts = Vec::new();
    loop {
        state = match state {
            CounterNameState::FirstPart => {
                cursor.enter("counter-name.first-part");
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("a counter name"))?;
                cursor.require(
                    is_counter_name_part(token),
                    "a contextually resolved counter name",
                )?;
                parts.push(token.raw.clone());
                cursor.advance_to(cursor.position() + 1)?;
                CounterNameState::MoreParts
            }
            CounterNameState::MoreParts => {
                cursor.enter("counter-name.more-parts");
                if cursor.peek_token().is_some_and(is_counter_name_part) {
                    let token = cursor.peek_token().expect("checked counter-name part");
                    parts.push(token.raw.clone());
                    cursor.advance_to(cursor.position() + 1)?;
                    CounterNameState::MoreParts
                } else {
                    CounterNameState::Complete
                }
            }
            CounterNameState::Complete => {
                cursor.enter("counter-name.complete");
                cursor.require(
                    cursor.peek().is_some_and(|raw| {
                        raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters")
                    }),
                    "`counter` or `counters` after the counter name",
                )?;
                return CounterName::from_parts(parts.iter().map(String::as_str))
                    .ok_or_else(|| cursor.fail("a non-empty canonical counter name"));
            }
        };
    }
}

/// A counter name whose `counter(s)` noun is shared with a later option of
/// a coordinated list (`a +1/+1, first strike, or trample counter`). The lexer
/// classifies only the span right before the noun, so each earlier name is
/// read here with the lexer's same token-class check, and ends at `,` or
/// `or`, which this mode leaves to the list.
pub(in crate::oracle::canonical) fn recognize_counter_name_sharing_noun(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterName> {
    cursor.enter("counter-name.shared-noun");
    let mut parts = Vec::new();
    loop {
        let token = cursor
            .peek_token()
            .ok_or_else(|| cursor.fail("a counter name before `,` or `or`"))?;
        if token.raw == "," || token.raw.eq_ignore_ascii_case("or") {
            break;
        }
        cursor.require(
            is_counter_name_part(token)
                || crate::oracle::syntax::is_counter_name_part_candidate(token)
                || crate::oracle::syntax::is_keyword_counter_name_head(
                    &token.raw,
                    cursor.peek_ahead(1),
                ),
            "a counter name part before `,` or `or`",
        )?;
        parts.push(token.raw.clone());
        cursor.advance_to(cursor.position() + 1)?;
    }
    cursor.enter("counter-name.shared-noun.complete");
    CounterName::from_parts(parts.iter().map(String::as_str))
        .ok_or_else(|| cursor.fail("a non-empty canonical counter name"))
}

fn is_counter_name_part(token: &crate::oracle::model::RecognizedEntity) -> bool {
    matches!(
        token.token_type.as_str(),
        "counterNamePart" | "counterValue"
    )
}

#[cfg(test)]
#[path = "counter_name_tests.rs"]
mod tests;
