use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_conditional_entry_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let this_type_source = tokens.len() >= 2
        && tokens[0].raw.eq_ignore_ascii_case("this")
        && tokens[1].token_type == "typeWord";
    let named_source = !tokens.is_empty()
        && !tokens[0].raw.eq_ignore_ascii_case("this")
        && source_reference_starts_at(
            0,
            Some(tokens[0].raw.as_str()),
            input.name_links,
            input.name_resolution_failure,
        );
    (this_type_source || named_source)
        && tokens.windows(6).any(|window| {
            window[0].raw.eq_ignore_ascii_case("enters")
                && window[1].raw.eq_ignore_ascii_case("tapped")
                && window[2].raw.eq_ignore_ascii_case("unless")
                && window[3].raw.eq_ignore_ascii_case("you")
                && window[4].raw.eq_ignore_ascii_case("control")
                && (window[5].raw.eq_ignore_ascii_case("a")
                    || window[5].raw.eq_ignore_ascii_case("an"))
        })
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
    cursor.require(
        domain_filter_is_representable(&node.required_permanent),
        "a controlled permanent predicate representable without erasing a Boolean branch",
    )?;
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
