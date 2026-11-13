use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the generic permanent-scaled self-casting-cost reduction envelope.
/// Amount, selector, scope, and punctuation remain owned by the token FSM so
/// malformed members of this family fail natively instead of falling back.
pub(in crate::oracle::canonical) fn claims_promoted_permanent_scaled_casting_reduction_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    matches!(tokens, [first, second, third, ..]
        if first.raw.eq_ignore_ascii_case("this")
            && second.raw.eq_ignore_ascii_case("spell")
            && third.raw.eq_ignore_ascii_case("costs"))
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("less")
                && window[1].raw.eq_ignore_ascii_case("to")
                && window[2].raw.eq_ignore_ascii_case("cast")
        })
        && tokens.windows(3).any(|window| {
            window[0].raw.eq_ignore_ascii_case("for")
                && (window[1].raw.eq_ignore_ascii_case("each")
                    || window[1].raw.eq_ignore_ascii_case("every"))
                && matches!(
                    window[2].raw.to_ascii_lowercase().as_str(),
                    "artifact"
                        | "artifacts"
                        | "battle"
                        | "battles"
                        | "creature"
                        | "creatures"
                        | "enchantment"
                        | "enchantments"
                        | "land"
                        | "lands"
                        | "planeswalker"
                        | "planeswalkers"
                )
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_permanent_scaled_casting_reduction(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("permanent-scaled-casting-reduction", input.tokens);
    let reduction = recognize_permanent_scaled_casting_reduction(&mut cursor)?;
    let count = json!({
        "kind": "countPermanents",
        "allPlayers": true,
        "where": lower_permanent_object_predicate(&reduction.selector),
    });
    let amount = if reduction.generic_reduction_per_permanent == 1 {
        count
    } else {
        json!({
            "kind": "multiply",
            "left": integer(reduction.generic_reduction_per_permanent),
            "right": count,
        })
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
            "activeWhile": {
                "kind": "inZone",
                "object": self_ref(),
                "zone": { "kind": "stackOrCast" },
            },
            "modifiers": [{
                "kind": "reduceOwnGenericCastingCost",
                "amount": amount,
            }],
        }),
        &[
            "Recognize a permanent-scaled generic casting-cost reduction",
            "Delegate the permanent predicate to the shared selector automaton",
            "Count matching permanents across all battlefields",
            "Lower scaling and reduction as independent primitive expressions",
            &trace,
        ],
    ))
}
