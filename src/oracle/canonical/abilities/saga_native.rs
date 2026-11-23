use super::super::*;
use crate::oracle::syntax::AbilityInput;

fn is_saga_type_line(type_line: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("Saga"))
}

fn is_roman_shaped(raw: &str) -> bool {
    !raw.is_empty()
        && raw
            .chars()
            .all(|character| matches!(character, 'I' | 'V' | 'X'))
}

/// Claims only the standardized reminder or a Roman chapter header on a Saga
/// type line. The type boundary prevents unrelated Roman-labelled abilities
/// from being stolen from legacy parsers.
pub(in crate::oracle::canonical) fn claims_promoted_saga_family(input: &AbilityInput<'_>) -> bool {
    if !is_saga_type_line(input.face_type_line) {
        return false;
    }
    let tokens = input.tokens;
    let reminder = tokens.get(0..4).is_some_and(|prefix| {
        prefix[0].raw == "("
            && prefix[1].raw.eq_ignore_ascii_case("as")
            && prefix[2].raw.eq_ignore_ascii_case("this")
            && prefix[3].raw.eq_ignore_ascii_case("Saga")
    });
    let chapter = tokens
        .first()
        .is_some_and(|token| is_roman_shaped(&token.raw));
    reminder || chapter
}

pub(in crate::oracle::canonical) fn parse_promoted_saga(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let reminder = input.tokens.first().is_some_and(|token| token.raw == "(");
    if reminder {
        let mut cursor = OracleTokenCursor::new("saga-progression-reminder", input.tokens);
        let reminder = recognize_saga_progression_reminder(&mut cursor)?;
        return Ok(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "marker": "sagaProgression",
                "finalChapter": integer(reminder.final_chapter),
            }),
            &[
                "Recognize the Saga progression reminder from tokens",
                "Retain the final Roman chapter as typed documentary metadata",
                "Leave runtime progression driven by chapter rules",
            ],
        ));
    }

    let mut cursor = OracleTokenCursor::new("saga-chapter", input.tokens);
    let chapter =
        recognize_saga_chapter(&mut cursor, input.name_links, input.name_resolution_failure)?;
    // The chapter body is lowered exactly as a triggered ability's body;
    // its target declarations and modes belong to the chapter trigger.
    let lowered = match &chapter.body {
        TriggerBodyNode::Program(program) => lower_effect_program(&mut cursor, program)?,
        TriggerBodyNode::Modal(modal) => lower_effect_sequence_modal(&mut cursor, modal)?,
    };
    cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable action",
    )?;
    let decisions = lowered.decisions;
    let effects = lowered.effects;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": {
            "kind": "sagaChapterReached",
            "object": self_ref(),
            "chapters": chapter.chapters.into_iter().map(integer).collect::<Vec<_>>(),
        },
        "effects": effects,
    });
    if !decisions.is_empty() {
        rule["declaration"] = json!({ "kind": "castingDeclaration", "decisions": decisions });
    }
    Ok(draft(
        rule,
        &[
            "Recognize one or more Saga chapter numerals from the shared token stream",
            "Delegate the chapter body to the shared container body slot",
            "Lower the chapter event and body to reusable primitive IR",
            &trace,
        ],
    ))
}
