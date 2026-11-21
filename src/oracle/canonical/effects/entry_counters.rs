use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

/// Claims a resolved source reference followed by `enters with`. The child
/// automaton validates quantity, counter kind, recipient and terminator, so a
/// condition or unsupported quantity fails at a precise state.
pub(in crate::oracle::canonical) fn claims_promoted_source_entry_counters_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("source-entry-counters-claim", input.tokens);
    if !matches!(
        recognize_source_reference(&mut cursor, input.name_links, input.name_resolution_failure),
        Ok(LocalObjectReference::SourceSelf)
    ) {
        return false;
    }
    let position = cursor.position();
    input
        .tokens
        .get(position)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("enters"))
        && input
            .tokens
            .get(position + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("with"))
}

pub(in crate::oracle::canonical) fn parse_promoted_source_entry_counters(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-entry-counters", input.tokens);
    let node = recognize_source_entry_counters(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("replacement.entry-counters.validation");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent card face for an entering source",
    )?;
    let count = cursor.require_value(
        lower_numeric_expression(&node.count),
        "a counter quantity the entry replacement can evaluate",
    )?;
    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
            "replacement": [{
                "kind": "putEnteringCounters",
                "counter": node.counter.as_str(),
                "count": count,
            }],
        }),
        &[
            "Recognize an entering source and its counter quantity",
            "Compose the shared counter name and numeric quantity",
            "Apply one primitive entering-counter replacement",
        ],
    ))
}

#[cfg(test)]
#[path = "entry_counters_tests.rs"]
mod tests;
