use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn token_is(input: &AbilityInput<'_>, index: usize, expected: &str) -> bool {
    input
        .tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

/// Claims only the bounded `When this SOURCE enters, exile TARGET until this
/// SOURCE leaves the battlefield.` envelope. Selector vocabulary remains
/// deliberately unconstrained here so a committed child failure is surfaced.
pub(in crate::oracle::canonical) fn claims_promoted_linked_entry_exile_family(
    input: &AbilityInput<'_>,
) -> bool {
    if !token_is(input, 0, "when")
        || !token_is(input, 1, "this")
        || !token_is(input, 3, "enters")
        || !token_is(input, 4, ",")
        || !token_is(input, 5, "exile")
    {
        return false;
    }

    let Some(duration_index) =
        input
            .tokens
            .iter()
            .enumerate()
            .skip(6)
            .find_map(|(index, token)| {
                (token.token_type == "durationBoundaryWord"
                    && token.raw.eq_ignore_ascii_case("until"))
                .then_some(index)
            })
    else {
        return false;
    };
    let target_is_present = input.tokens[6..duration_index]
        .iter()
        .any(|token| token.token_type == "targetMarker");

    target_is_present
        && duration_index + 7 == input.tokens.len()
        && token_is(input, duration_index + 1, "this")
        && token_is(input, duration_index + 3, "leaves")
        && token_is(input, duration_index + 4, "the")
        && token_is(input, duration_index + 5, "battlefield")
        && token_is(input, duration_index + 6, ".")
}

pub(in crate::oracle::canonical) fn parse_promoted_linked_entry_exile(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("linked-entry-exile", input.tokens);
    let _node = recognize_source_entry_linked_exile(&mut cursor)?;
    cursor.enter("linked-entry-exile.generic-relation-unavailable");
    Err(cursor.fail("proved generic moveCards and durable source-incarnation relation primitives"))
}
