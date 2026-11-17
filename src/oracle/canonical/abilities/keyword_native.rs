use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the promoted mana-Kicker family while leaving distinct multi-cost
/// and nonmana Kicker grammars at their explicit migration boundaries.
pub(in crate::oracle::canonical) fn claims_promoted_mana_kicker_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if !tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("kicker"))
        || !tokens
            .get(1)
            .is_some_and(|token| token.token_type == "manaSymbol")
    {
        return false;
    }

    let first_nonmana = tokens
        .iter()
        .skip(1)
        .position(|token| token.token_type != "manaSymbol")
        .map(|offset| offset + 1);
    !first_nonmana
        .and_then(|index| tokens.get(index))
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("and/or"))
}

pub(in crate::oracle::canonical) fn parse_promoted_mana_kicker(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-kicker", input.tokens);
    let kicker = recognize_mana_kicker(&mut cursor)?;
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
                "kind": "kicker",
                "cost": {
                    "kind": "payMana",
                    "manaCost": kicker.cost.as_oracle_cost(),
                },
                "repeatable": false,
            },
        }),
        &[
            "Recognize the Kicker keyword from the shared token stream",
            "Delegate its payment to the mana-cost automaton",
            "Validate the optional reminder against the printed cost",
            &trace,
        ],
    ))
}
