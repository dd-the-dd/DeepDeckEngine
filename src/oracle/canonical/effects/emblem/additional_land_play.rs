use crate::oracle::syntax::AbilityInput;

use super::super::super::*;
use super::lower_player_modifier;

pub(in crate::oracle::canonical) fn claims_promoted_additional_land_play_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.get(0..3).is_some_and(|prefix| {
        prefix[0].raw.eq_ignore_ascii_case("you")
            && prefix[1].raw.eq_ignore_ascii_case("may")
            && prefix[2].raw.eq_ignore_ascii_case("play")
    }) && input.tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("additional")
            && (window[1].raw.eq_ignore_ascii_case("land")
                || window[1].raw.eq_ignore_ascii_case("lands"))
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_additional_land_play(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-additional-land-play", input.tokens);
    let modifier = recognize_static_additional_land_play(&mut cursor)?;
    let PlayerModifierNode::AdditionalLandPlay { player, count } = &modifier else {
        return Err(cursor.fail("an additional-land player modifier"));
    };
    cursor.enter("modifier.additional-land-play.lower.player");
    cursor.require(
        *player == LocalPlayerReference::Controller,
        "the ability controller as the additional-land player",
    )?;
    cursor.enter("modifier.additional-land-play.lower.count");
    cursor.require(
        matches!(
            count,
            NumericExpression::Atom(NumericAtom::Integer(value))
                if *value > 0 && *value <= i64::from(i32::MAX)
        ),
        "a positive i32 literal additional-land count",
    )?;
    let lowered = lower_player_modifier(&modifier)
        .ok_or_else(|| cursor.fail("a runtime-supported additional-land count"))?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [lowered],
        }),
        &[
            "Recognize the affected player through the shared player-reference automaton",
            "Recognize the allowance through the shared numeric-expression automaton",
            "Keep the controller-relative per-turn period in the static parent",
            "Lower through the existing additionalLandPlay primitive",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "additional_land_play_tests.rs"]
mod tests;
