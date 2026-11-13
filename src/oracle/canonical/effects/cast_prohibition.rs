use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims only the opponent spell-casting prohibition family. Malformed turn
/// contexts remain owned by the native machine instead of falling through to
/// an exact-text legacy branch.
pub(in crate::oracle::canonical) fn claims_promoted_cast_prohibition_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("your"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("opponents"))
        && tokens.windows(3).any(|window| {
            (window[0].raw.eq_ignore_ascii_case("can't")
                || window[0].raw.eq_ignore_ascii_case("cannot"))
                && window[1].raw.eq_ignore_ascii_case("cast")
                && window[2].raw.eq_ignore_ascii_case("spells")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_cast_prohibition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("cast-prohibition", input.tokens);
    let prohibition = recognize_cast_prohibition(&mut cursor)?;
    cursor.enter("cast-prohibition.lower.players");
    let players = match prohibition.players {
        PlayerSetReferenceNode::OpponentsOfAbilityController => json!({
            "kind": "opponentsOf",
            "player": controller(),
        }),
    };
    cursor.enter("cast-prohibition.lower.turn");
    let during_turn_of = match prohibition.turn {
        TurnContextNode::AbilityControllerTurn => controller(),
    };
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
            "modifiers": [{
                "kind": "prohibitCast",
                "players": players,
                "duringTurnOf": during_turn_of,
            }],
        }),
        &[
            "Recognize a typed opponent player set",
            "Recognize the controller's turn as an independent timing context",
            "Lower the relation to the generic prohibitCast modifier",
            &trace,
        ],
    ))
}
