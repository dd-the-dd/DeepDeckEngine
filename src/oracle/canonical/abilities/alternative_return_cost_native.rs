use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a controlled-permanent return used as a replacement for the
/// current spell's mana cost. Child automata own the selector, controller
/// relation, owner destination, and replaced-cost reference.
pub(in crate::oracle::canonical) fn claims_promoted_alternative_return_cost_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens.len() >= 8
        && tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("you"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("may"))
        && tokens
            .get(2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("return"))
        && tokens.windows(2).any(|window| {
            window[0].raw.eq_ignore_ascii_case("rather")
                && window[1].raw.eq_ignore_ascii_case("than")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_alternative_return_cost(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("alternative-return-cost", input.tokens);
    let envelope = recognize_alternative_return_cost_envelope(&mut cursor)?;
    let CurrentSpellManaCostReferenceNode = envelope.replaces;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": {
                "kind": "alternativeCost",
                "costs": [{
                    "kind": "returnPermanentToOwnersHand",
                    "where": lower_permanent_object_predicate(&envelope.cost.predicate),
                }],
            },
        }),
        &[
            "Recognize an alternative-mana-cost permission from the shared token stream",
            "Delegate the permanent predicate and controller relation to the object query child",
            "Bind the destination to the selected permanent's owner hand",
            "Lower one independent return-permanent cost primitive",
            &trace,
        ],
    ))
}
