use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_mana_overload_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("overload"))
        && input
            .tokens
            .get(1)
            .is_some_and(|token| token.token_type == "manaSymbol")
}

pub(in crate::oracle::canonical) fn parse_promoted_mana_overload(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-overload", input.tokens);
    let overload = recognize_mana_overload(&mut cursor)?;
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
                "mode": "overload",
                "costs": [{
                    "kind": "payMana",
                    "manaCost": overload.cost.as_oracle_cost(),
                }],
                "compileDirective": {
                    "kind": "replaceTargetWithEach",
                },
            },
        }),
        &[
            "Recognize Overload from the shared token stream",
            "Delegate its alternative payment to the mana-cost automaton",
            "Validate the canonical reminder token by token",
            "Emit a structured target-to-each compilation directive",
            &trace,
        ],
    ))
}
