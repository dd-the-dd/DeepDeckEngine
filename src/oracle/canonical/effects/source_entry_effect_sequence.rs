use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the promoted part of the source-entry envelope without swallowing
/// unrelated legacy ETB bodies. The parent is generic, while the initial
/// promotion boundary is the already-native draw or temporary P/T sequence.
/// Once claimed, malformed children remain committed native failures.
pub(in crate::oracle::canonical) fn claims_promoted_source_entry_effect_sequence_family(
    input: &AbilityInput<'_>,
) -> bool {
    let mut cursor = OracleTokenCursor::new("source-entry-effect-sequence-claim", input.tokens);
    if cursor.consume("when").is_err() {
        return false;
    }
    let Ok(LocalTriggerEventNode::SourceEntered { source }) = recognize_source_entered_event(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) else {
        return false;
    };
    if source != LocalObjectReference::SourceSelf || cursor.consume(",").is_err() {
        return false;
    }

    let body_start = cursor.position();
    let Some(first) = input.tokens.get(body_start) else {
        return false;
    };
    if first.raw.eq_ignore_ascii_case("draw") {
        return true;
    }
    if first.raw.eq_ignore_ascii_case("you")
        && input
            .tokens
            .get(body_start + 1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("may"))
        && input
            .tokens
            .get(body_start + 2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("return"))
        && input.tokens[body_start + 3..].windows(2).any(|window| {
            (window[0].raw.eq_ignore_ascii_case("card")
                || window[0].raw.eq_ignore_ascii_case("cards"))
                && window[1].raw.eq_ignore_ascii_case("from")
        })
    {
        return true;
    }

    let body = AbilityInput {
        face_name: input.face_name,
        face_type_line: input.face_type_line,
        source: input.source,
        tokens: &input.tokens[body_start..],
        name_links: input.name_links,
        name_resolution_failure: input.name_resolution_failure,
    };
    claims_promoted_temporary_power_toughness_family(&body)
}

pub(in crate::oracle::canonical) fn parse_promoted_source_entry_effect_sequence(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("source-entry-effect-sequence", input.tokens);
    let node = recognize_source_entry_effect_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.require(
        node.source == LocalObjectReference::SourceSelf,
        "the current source entering",
    )?;
    let lowered = lower_local_effect_sequence(&mut cursor, &node.effects)?;
    cursor.require(
        !lowered.effects.is_empty(),
        "at least one executable atomic effect",
    )?;

    let effects = if node.optional {
        vec![json!({
            "kind": "optionalEffects",
            "player": controller(),
            "effects": lowered.effects,
        })]
    } else {
        lowered.effects
    };
    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": { "kind": "enterBattlefield", "object": self_ref() },
        "effects": effects,
    });
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        rule,
        &[
            "Recognize the source-entry event through the shared event automaton",
            "Delegate the complete body to the shared EffectSequence",
            "Preserve target declarations and anaphoric bindings from sequence lowering",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "source_entry_effect_sequence_tests.rs"]
mod tests;
