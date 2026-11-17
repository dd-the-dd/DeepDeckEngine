use super::super::*;
use crate::oracle::syntax::AbilityInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaimedCostBranch {
    DiscardCard,
    PayLife,
}

/// Claims the token-shaped mandatory additional-cost family whose alternatives
/// are one card discard and one fixed life payment. The child machine owns all
/// syntax and committed diagnostics after this structural boundary.
pub(in crate::oracle::canonical) fn claims_promoted_alternative_additional_cost_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let envelope = MANDATORY_ADDITIONAL_COST_HEADER;
    if !envelope.iter().enumerate().all(|(index, expected)| {
        tokens
            .get(index)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
    }) || !tokens.last().is_some_and(|token| token.raw == ".")
    {
        return false;
    }
    let body = &tokens[envelope.len()..tokens.len() - 1];
    let connectors = body
        .iter()
        .enumerate()
        .filter_map(|(index, token)| token.raw.eq_ignore_ascii_case("or").then_some(index))
        .collect::<Vec<_>>();
    let [connector] = connectors.as_slice() else {
        return false;
    };
    let left = claim_cost_branch(&body[..*connector]);
    let right = claim_cost_branch(&body[*connector + 1..]);
    matches!(
        (left, right),
        (
            Some(ClaimedCostBranch::DiscardCard),
            Some(ClaimedCostBranch::PayLife)
        ) | (
            Some(ClaimedCostBranch::PayLife),
            Some(ClaimedCostBranch::DiscardCard)
        )
    )
}

fn claim_cost_branch(
    tokens: &[crate::oracle::model::RecognizedEntity],
) -> Option<ClaimedCostBranch> {
    let first = tokens.first()?.raw.to_ascii_lowercase();
    match first.as_str() {
        "discard" => Some(ClaimedCostBranch::DiscardCard),
        "pay"
            if tokens
                .iter()
                .any(|token| token.raw.eq_ignore_ascii_case("life")) =>
        {
            Some(ClaimedCostBranch::PayLife)
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_alternative_additional_cost(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("alternative-additional-cost", input.tokens);
    let choice = recognize_alternative_additional_cost(&mut cursor)?;
    cursor.enter("additional-cost.alternative.mode-validation");
    let mut modes = Vec::with_capacity(choice.options.len());
    for option in &choice.options {
        let mode = option.mode_name();
        cursor.require(
            !modes.contains(&mode),
            "distinct alternative additional-cost modes",
        )?;
        modes.push(mode);
    }

    let mut decisions = vec![json!({
        "id": "additionalCostMode",
        "kind": "chooseModes",
        "minimum": 1,
        "maximum": 1,
        "options": modes,
    })];
    let mut additional_costs = Vec::with_capacity(choice.options.len());
    let mut discard_index = 0;
    for option in choice.options {
        let mode = option.mode_name();
        let condition = selection("additionalCostMode", mode);
        let cost = match option {
            AlternativeAdditionalCostOptionNode::DiscardCard(_) => {
                discard_index += 1;
                let decision_id = format!("discardCost{discard_index}");
                let mut decision = target_decision(
                    &decision_id,
                    json!({
                        "kind": "cards",
                        "zone": { "kind": "hand", "player": controller() },
                        "where": Value::Null,
                    }),
                    1,
                    1,
                );
                decision["condition"] = condition.clone();
                decisions.push(decision);
                json!({
                    "kind": "discardCard",
                    "card": chosen_target(&decision_id),
                })
            }
            AlternativeAdditionalCostOptionNode::PayLife(pay_life) => json!({
                "kind": "payLife",
                "player": controller(),
                "amount": integer(pay_life.amount),
            }),
        };
        additional_costs.push(json!({
            "kind": "conditional",
            "condition": condition,
            "then": [cost],
            "else": [],
        }));
    }

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
                "additionalCosts": additional_costs,
            },
            "effects": [],
        }),
        &[
            "Recognize the mandatory additional-cost envelope from one token stream",
            "Delegate discard-card and pay-life syntax to independent typed cost atoms",
            "Model `or` as one casting-time mode selection",
            "Condition each declaration and payment on exactly one chosen mode",
            &trace,
        ],
    ))
}
