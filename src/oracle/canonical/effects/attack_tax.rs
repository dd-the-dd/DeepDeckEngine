use crate::oracle::syntax::AbilityInput;

use super::super::*;

pub(in crate::oracle::canonical) fn claims_promoted_attack_tax_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.get(0..3).is_some_and(|prefix| {
        prefix[0].raw.eq_ignore_ascii_case("creatures")
            && (prefix[1].raw.eq_ignore_ascii_case("can't")
                || prefix[1].raw.eq_ignore_ascii_case("can’t"))
            && prefix[2].raw.eq_ignore_ascii_case("attack")
    }) && input.tokens.windows(7).any(|window| {
        window[0].raw.eq_ignore_ascii_case("for")
            && window[1].raw.eq_ignore_ascii_case("each")
            && window[2].raw.eq_ignore_ascii_case("creature")
            && window[3].raw.eq_ignore_ascii_case("they")
            && window[4].raw.eq_ignore_ascii_case("control")
            && (window[5].raw.eq_ignore_ascii_case("that's")
                || window[5].raw.eq_ignore_ascii_case("that’s"))
            && window[6].raw.eq_ignore_ascii_case("attacking")
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_attack_tax(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-attack-tax", input.tokens);
    let modifier = recognize_attack_tax_modifier(&mut cursor)?;
    cursor.enter("modifier.attack-tax.lower.player");
    cursor.require(
        modifier.protected_player == LocalPlayerReference::Controller,
        "the ability controller as protected player",
    )?;
    cursor.enter("modifier.attack-tax.lower.payer");
    cursor.require(
        modifier.payer == AttackTaxPayerNode::AttackingCreatureController,
        "the attacking creature's controller as payer",
    )?;
    let amount = lower_numeric_expression(&modifier.amount)
        .ok_or_else(|| cursor.fail("a runtime-supported numeric attack tax"))?;
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
                "kind": "attackTax",
                "protectedPlayer": controller(),
                "amount": amount,
            }],
        }),
        &[
            "Recognize the creature attacker domain from typed tokens",
            "Bind the protected player through the shared player-reference automaton",
            "Retain the attacking creature's controller as the payer relation",
            "Lower the generic mana payment through the shared numeric-expression IR",
            "Emit the existing attackTax runtime modifier",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "attack_tax_tests.rs"]
mod tests;
