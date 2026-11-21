use super::super::*;
use crate::oracle::syntax::AbilityInput;

const COUNTERED_SPELL_CONTROLLER_BINDING: &str = "counteredSpellController";

pub(in crate::oracle::canonical) fn claims_promoted_counter_delayed_draw_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 12
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("counter"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens.windows(5).any(|window| {
            window[0].raw == "."
                && window[1].raw.eq_ignore_ascii_case("its")
                && window[2].raw.eq_ignore_ascii_case("controller")
                && window[3].raw.eq_ignore_ascii_case("may")
                && window[4].raw.eq_ignore_ascii_case("draw")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_counter_delayed_draw(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("counter-delayed-draw", input.tokens);
    let sequence = recognize_counter_delayed_draw(&mut cursor)?;
    cursor.enter("sequence.counter-delayed-draw.validation");
    let antecedent = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    let mut draft = lower_counter_effect(&sequence.counter, cursor.transitions(), false);
    let target_id = draft.rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .ok_or_else(|| cursor.fail("a lowered counter target decision"))?
        .to_string();
    let counter = draft.rule["effects"]
        .as_array_mut()
        .and_then(|effects| (!effects.is_empty()).then(|| effects.remove(0)))
        .ok_or_else(|| cursor.fail("a lowered atomic counter effect"))?;
    let delayed = lower_delayed_draw_schedule(
        &cursor,
        &sequence.delayed,
        Some((antecedent, COUNTERED_SPELL_CONTROLLER_BINDING)),
    )?;
    draft.rule["effects"] = json!([
        {
            "kind": "bind",
            "id": COUNTERED_SPELL_CONTROLLER_BINDING,
            "value": {
                "kind": "controllerOf",
                "object": chosen_target(&target_id),
            },
        },
        counter,
        delayed,
    ]);
    draft.operations = vec![
        "Capture the counter target controller before the target leaves the stack".to_string(),
        "Counter the chosen spell with the atomic counter primitive".to_string(),
        "Install a next-turn delayed draw with a transported player binding".to_string(),
        cursor
            .transitions()
            .iter()
            .map(|transition| transition.state)
            .collect::<Vec<_>>()
            .join(" -> "),
    ];
    Ok(draft)
}
