use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

/// Claims a resolved source-reference prefix of an unconditional state-entry
/// replacement. The child automaton remains responsible for validating the
/// complete source reference, permanent context, state, and terminator.
pub(in crate::oracle::canonical) fn claims_promoted_source_entry_state_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("source-entry-state-claim", input.tokens);
    if !matches!(
        recognize_source_reference(&mut cursor, input.name_links, input.name_resolution_failure,),
        Ok(LocalObjectReference::SourceSelf)
    ) {
        return false;
    }
    let Some(event) = input.tokens.get(cursor.position()) else {
        return false;
    };
    let Some(state) = input.tokens.get(cursor.position() + 1) else {
        return false;
    };
    if !event.raw.eq_ignore_ascii_case("enters")
        || (!state.raw.eq_ignore_ascii_case("tapped")
            && !state.raw.eq_ignore_ascii_case("prepared"))
    {
        return false;
    }
    let state_end = cursor.position() + 2;
    if !input
        .tokens
        .get(state_end)
        .is_some_and(|terminator| terminator.raw == ".")
    {
        return false;
    }
    let trailing = &input.tokens[state_end + 1..];
    trailing.is_empty()
        || (state.raw.eq_ignore_ascii_case("prepared")
            && is_canonical_prepare_reminder_tokens(trailing))
}

fn is_canonical_prepare_reminder_tokens(tokens: &[crate::oracle::model::RecognizedEntity]) -> bool {
    let expected = [
        "(",
        "while",
        "it's",
        "prepared",
        ",",
        "you",
        "may",
        "cast",
        "a",
        "copy",
        "of",
        "its",
        "spell",
        ".",
        "doing",
        "so",
        "unprepares",
        "it",
        ".",
        ")",
    ];
    tokens.len() == expected.len()
        && tokens
            .iter()
            .zip(expected)
            .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected))
}

pub(in crate::oracle::canonical) fn parse_promoted_source_entry_state(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-entry-state", input.tokens);
    let node =
        recognize_source_entry_state(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("replacement.source-entry-state.validation");
    cursor.require(
        node.source == LocalObjectReference::SourceSelf,
        "an entering state applied to the current source",
    )?;
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent card face for an entering source",
    )?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let replacement = match node.state {
        SourceEntryState::Tapped => json!({
            "kind": "setEnteringState",
            "object": self_ref(),
            "tapped": true,
        }),
        SourceEntryState::Prepared => json!({
            "kind": "setPrepared",
            "object": self_ref(),
            "value": true,
        }),
    };
    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": self_ref(),
            },
            "replacement": [replacement],
        }),
        &[
            "Recognize an explicitly typed source entering the battlefield",
            "Apply one atomic typed entering-state replacement",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "source_entry_tapped_tests.rs"]
mod tests;
