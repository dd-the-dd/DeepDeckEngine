use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// One payment of a keyword cost parameter (`Keyword—COSTS`, CR 702.1)
/// lowered to the keyword-cost schema the casting and activation runtime
/// pays (`payMana`, `payLife`, and one chosen permanent or card by filter).
/// A payment that schema cannot carry fails explicitly; it is never dropped.
pub(in crate::oracle::canonical) fn lower_keyword_cost_payment(
    cursor: &OracleTokenCursor<'_>,
    cost: &LocalActivationCostNode,
) -> MachineResult<Value> {
    let single = |count: i64, exclude_source: bool| {
        cursor.require(
            count == 1 && !exclude_source,
            "one chosen object per keyword payment (several or `another` have no keyword-cost runtime)",
        )
    };
    Ok(match cost {
        LocalActivationCostNode::PayMana(mana) => {
            mana.fixed_runtime_mana_value()
                .map_err(|error| cursor.fail(error.expected()))?;
            lower_resource_mana_payment(mana)
        }
        LocalActivationCostNode::PayLife(life) => {
            lower_resource_life_payment(life, ResourcePaymentPayer::ActivationController)
        }
        LocalActivationCostNode::SacrificeObject(sacrifice) => {
            single(sacrifice.count, sacrifice.exclude_source)?;
            json!({
                "kind": "sacrificePermanent",
                "where": lower_permanent_object_predicate(&sacrifice.object.predicate),
            })
        }
        LocalActivationCostNode::DiscardCard(discard)
            if discard.method == CollectionSelectionMethod::Choice =>
        {
            single(discard.count, false)?;
            json!({
                "kind": "discardCard",
                "where": lower_card_object_predicate(&discard.filter),
            })
        }
        LocalActivationCostNode::TapObjects(tap) => {
            single(tap.tap.count, tap.tap.exclude_source)?;
            json!({
                "kind": "tap",
                "where": lower_permanent_object_predicate(&tap.tap.object.predicate),
            })
        }
        _ => {
            return Err(cursor.fail(
                "a keyword payment the runtime pays: mana, life, or one chosen permanent to sacrifice or tap or card to discard",
            ));
        }
    })
}

/// A keyword cost parameter as one payment, or a `compositeCost` paid in
/// printed order (CR 601.2h).
pub(in crate::oracle::canonical) fn lower_keyword_cost_sequence(
    cursor: &OracleTokenCursor<'_>,
    costs: &ActivationCostSequenceNode,
) -> MachineResult<Value> {
    let mut lowered = costs
        .costs
        .iter()
        .map(|cost| lower_keyword_cost_payment(cursor, cost))
        .collect::<MachineResult<Vec<_>>>()?;
    Ok(match lowered.len() {
        0 => return Err(cursor.fail("a keyword cost with at least one payment")),
        1 => lowered.remove(0),
        _ => json!({ "kind": "compositeCost", "costs": lowered }),
    })
}

/// Claims a line whose first token is a registered parameterized casting
/// keyword. Splice additionally requires its `onto` relation, so an unrelated
/// sentence that merely starts with the same word is never intercepted.
pub(in crate::oracle::canonical) fn claims_promoted_cost_keyword_family(
    input: &AbilityInput<'_>,
) -> bool {
    let Some(keyword) = input
        .tokens
        .first()
        .and_then(|token| CostKeyword::from_head(&token.raw))
    else {
        return false;
    };
    let next = input.tokens.get(1);
    match keyword {
        CostKeyword::Splice => next.is_some_and(|token| token.raw.eq_ignore_ascii_case("onto")),
        CostKeyword::Dredge => next.is_some_and(|token| {
            !token.raw.is_empty() && token.raw.bytes().all(|byte| byte.is_ascii_digit())
        }),
        CostKeyword::Plot | CostKeyword::Foretell => {
            next.is_some_and(|token| token.token_type == "manaSymbol")
        }
    }
}

fn pay_mana(cost: &ManaCostNode) -> Value {
    json!({ "kind": "payMana", "manaCost": cost.as_oracle_cost() })
}

pub(in crate::oracle::canonical) fn parse_promoted_cost_keyword(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-cost-parameter", input.tokens);
    let node = recognize_cost_keyword(&mut cursor)?;
    cursor.enter("keyword.cost-keyword.lowering");
    let ability = match (node.keyword, &node.parameter) {
        (CostKeyword::Plot, CostKeywordParameter::Mana(cost)) => {
            json!({ "kind": "plot", "cost": pay_mana(cost) })
        }
        // CR 702.143a: the special action always costs {2}.
        (CostKeyword::Foretell, CostKeywordParameter::Mana(cost)) => json!({
            "kind": "foretell",
            "cost": pay_mana(cost),
            "exileCost": { "kind": "payMana", "manaCost": "{2}" },
        }),
        (CostKeyword::Dredge, CostKeywordParameter::Count(count)) => {
            json!({ "kind": "dredge", "count": integer(*count) })
        }
        (CostKeyword::Splice, CostKeywordParameter::SpliceOnto { quality, cost }) => {
            let onto = lower_spell_object_predicate(quality)
                .ok_or_else(|| cursor.fail("a spell quality with a runtime spell filter"))?;
            json!({ "kind": "splice", "onto": onto, "cost": pay_mana(cost) })
        }
        _ => return Err(cursor.fail("the parameter kind registered for this keyword")),
    };
    Ok(draft(
        json!({ "kind": "keywordAbility", "source": self_ref(), "ability": ability }),
        &[
            "Recognize a registered parameterized casting keyword from the shared token stream",
            "Read its typed parameter through the shared mana-cost, count, or spell-quality child",
            "Validate the optional reminder against the printed parameter",
            "Lower to the existing keyword primitive executed by the casting runtime",
        ],
    ))
}

#[cfg(test)]
#[path = "keyword_cost_tests.rs"]
mod keyword_cost_tests;
