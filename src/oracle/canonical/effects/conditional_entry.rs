use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_conditional_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 8
        && tokens[0].raw.eq_ignore_ascii_case("this")
        && tokens[1].token_type == "typeWord"
        && tokens[2].raw.eq_ignore_ascii_case("enters")
        && tokens[3].raw.eq_ignore_ascii_case("tapped")
        && tokens[4].raw.eq_ignore_ascii_case("unless")
        && tokens[5].raw.eq_ignore_ascii_case("you")
        && tokens[6].raw.eq_ignore_ascii_case("control")
        && (tokens[7].raw.eq_ignore_ascii_case("a") || tokens[7].raw.eq_ignore_ascii_case("an"))
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_entry(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("conditional-entry", input.tokens);
    let node = recognize_controlled_permanent_conditional_entry(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("replacement.conditional-entry.validation");
    cursor.require(
        node.source == LocalObjectReference::SourceSelf,
        "the current source entering the battlefield",
    )?;
    let _source_type = node.source_type;
    let predicate = lower_permanent_object_predicate(&node.required_permanent);

    Ok(draft(
        json!({
            "kind": "replacementEffect",
            "source": self_ref(),
            "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
            "condition": {
                "kind": "not",
                "operand": {
                    "kind": "controlsPermanent",
                    "player": controller(),
                    "where": predicate,
                },
            },
            "replacement": [{
                "kind": "setEnteringState",
                "object": self_ref(),
                "tapped": true,
            }],
        }),
        &[
            "Recognize source, entry state, controller, and permanent selector independently",
            "Lower the exception as one negated controlsPermanent condition",
            "Apply one atomic entering-state replacement",
        ],
    ))
}
