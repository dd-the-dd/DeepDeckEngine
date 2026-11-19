use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_mana_equip_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if !tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("equip"))
    {
        return false;
    }
    // `Equip {cost}` or the CR 702.6c `Equip [quality] creature {cost}` form.
    tokens
        .get(1)
        .is_some_and(|token| token.token_type == "manaSymbol")
        || tokens.windows(2).any(|pair| {
            pair[0].raw.eq_ignore_ascii_case("creature") && pair[1].token_type == "manaSymbol"
        })
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

    let mut ability = json!({
        "kind": "equip",
        "costs": [{
            "kind": "payMana",
            "manaCost": equip.cost.as_oracle_cost(),
        }],
    });
    if let Some(quality) = &equip.quality {
        ability["where"] = lower_permanent_object_predicate(&ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            quality.clone(),
        ]));
    }
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": ability,
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
