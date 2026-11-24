use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// A keyword line ending with `hexproof from QUALITIES` (CR 702.11d).
pub(in crate::oracle::canonical) fn claims_hexproof_from_family(input: &AbilityInput<'_>) -> bool {
    hexproof_from_line_starts(input.tokens)
}

/// The qualities reuse the protection quality reader and its lowering; the
/// keywords printed before it keep their own singular primitive.
pub(in crate::oracle::canonical) fn parse_hexproof_from_line(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-hexproof-from", input.tokens);
    let line = recognize_hexproof_from_line(&mut cursor)?;
    let mut abilities = line
        .keywords
        .iter()
        .map(|keyword| json!({ "kind": keyword.canonical_name() }))
        .collect::<Vec<_>>();
    abilities.push(json!({
        "kind": "hexproofFrom",
        "from": lower_protection_qualities(&line.qualities),
    }));
    let rule = if let [ability] = abilities.as_slice() {
        json!({ "kind": "keywordAbility", "source": self_ref(), "ability": ability })
    } else {
        json!({ "kind": "keywordAbilityGroup", "source": self_ref(), "abilities": abilities })
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        rule,
        &[
            "Recognize `hexproof from` after an optional parameterless keyword list",
            "Read each quality with the shared protection quality reader",
            "Validate an optional color reminder against the printed quality",
            &trace,
        ],
    ))
}

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
