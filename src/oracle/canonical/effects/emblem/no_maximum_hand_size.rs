use crate::oracle::syntax::AbilityInput;

use super::super::super::*;
use super::lower_player_modifier;

pub(in crate::oracle::canonical) fn claims_promoted_no_maximum_hand_size_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.get(0..3).is_some_and(|prefix| {
        prefix[0].raw.eq_ignore_ascii_case("you")
            && prefix[1].raw.eq_ignore_ascii_case("have")
            && prefix[2].raw.eq_ignore_ascii_case("no")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_no_maximum_hand_size(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-no-maximum-hand-size", input.tokens);
    let modifier = recognize_static_no_maximum_hand_size(&mut cursor)?;
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
            "modifiers": [lower_player_modifier(&modifier)],
        }),
        &[
            "Recognize the controller player reference from the shared token stream",
            "Recognize no maximum hand size as one typed player modifier",
            "Lower through the existing noMaximumHandSize primitive",
            &trace,
        ],
    ))
}
