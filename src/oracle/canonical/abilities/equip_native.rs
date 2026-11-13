use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_mana_equip_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("equip"))
        && input
            .tokens
            .get(1)
            .is_some_and(|token| token.token_type == "manaSymbol")
}

pub(in crate::oracle::canonical) fn parse_promoted_mana_equip(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-equip", input.tokens);
    let equip = recognize_mana_equip(&mut cursor)?;
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
                "kind": "equip",
                "costs": [{
                    "kind": "payMana",
                    "manaCost": equip.cost.as_oracle_cost(),
                }],
            },
        }),
        &[
            "Recognize Equip independently of any card name",
            "Delegate the printed activation cost to the shared mana-cost automaton",
            "Validate the optional attach-and-sorcery reminder structurally",
            "Lower to the existing primitive equip keyword contract",
            &trace,
        ],
    ))
}
