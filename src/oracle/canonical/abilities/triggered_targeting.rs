use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_became_target_trigger_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("whenever"))
        && input.tokens.get(1).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("a") || token.raw.eq_ignore_ascii_case("an")
        })
        && input.tokens.windows(4).any(|tokens| {
            tokens[0].raw.eq_ignore_ascii_case("becomes")
                && tokens[1].raw.eq_ignore_ascii_case("the")
                && tokens[2].raw.eq_ignore_ascii_case("target")
                && tokens[3].raw.eq_ignore_ascii_case("of")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_became_target_trigger(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("became-target-trigger", input.tokens);
    let ability = recognize_became_target_triggered_ability(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.became-target.effect-lowering");
    let effects = lower_targetless_trigger_effects(&ability.effects)
        .map_err(|error| cursor.fail(error.expectation()))?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": lower_became_target_event(&ability.event),
            "effects": effects,
        }),
        &[
            "Recognize the targeted-object selector union from the shared token stream",
            "Recognize the targeting stack-source selector independently",
            "Lower the ordered effect sequence to atomic primitives",
            &trace,
        ],
    ))
}
