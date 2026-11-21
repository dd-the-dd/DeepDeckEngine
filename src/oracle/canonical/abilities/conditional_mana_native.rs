use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims tap-led mana activations whose sentence suffix starts an activation
/// condition. The committed child owns all costs, mana alternatives,
/// predicates, articles, unions, and exact failure diagnostics.
pub(in crate::oracle::canonical) fn claims_promoted_conditional_mana_activation_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    if !tokens
        .first()
        .is_some_and(|token| token.token_type == "tapSymbol")
    {
        return false;
    }
    let Some(separator) = tokens.iter().position(|token| token.raw == ":") else {
        return false;
    };
    if !tokens
        .get(separator + 1)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("add"))
    {
        return false;
    }
    let Some(effect_terminator) = tokens
        .iter()
        .enumerate()
        .skip(separator + 2)
        .find_map(|(index, token)| (token.raw == ".").then_some(index))
    else {
        return false;
    };
    tokens
        .get(effect_terminator + 1)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("activate"))
        && tokens
            .get(effect_terminator + 2)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("only"))
        && tokens
            .get(effect_terminator + 3)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("if"))
}

fn lower_activation_condition(condition: &LocalActivationConditionNode) -> Value {
    match condition {
        LocalActivationConditionNode::ControlsPermanent { player, predicate } => {
            let player = match player {
                LocalPlayerReference::Controller => controller(),
                LocalPlayerReference::ControllerOf(_)
                | LocalPlayerReference::ControllerOfQualified(_) => {
                    unreachable!("activation condition binds the ability controller")
                }
            };
            json!({
                "kind": "controlsPermanent",
                "player": player,
                "where": lower_permanent_object_predicate(predicate),
            })
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_mana_activation(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("activated-mana-ability", input.tokens);
    let ability = recognize_conditional_activated_mana_ability(&mut cursor)?;
    require_supported_mana_cost_context(&mut cursor, &ability.costs)?;
    cursor.enter("activation.conditional-mana.validation");
    cursor.require(
        ability
            .costs
            .costs
            .iter()
            .any(|cost| matches!(cost, LocalActivationCostNode::TapSelf(_))),
        "a tap-self activation cost",
    )?;

    let costs = ability
        .costs
        .costs
        .iter()
        .flat_map(lower_local_activation_costs)
        .collect::<Vec<_>>();
    let activation_condition = lower_activation_condition(&ability.activation_condition);
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        with_activation_object_cost_decisions(
            json!({
                "kind": "manaAbility",
                "source": self_ref(),
                "activationCondition": activation_condition,
                "costs": costs,
                "effects": [lower_add_mana_effect(&ability.effect)],
            }),
            &ability.costs,
        ),
        &[
            "Recognize one mana activation from the shared token stream",
            "Bind an activation-only condition outside the effect sequence",
            "Compose an indefinite permanent-existence predicate with explicit union branches",
            "Lower to tap, controlsPermanent, and addMana primitives",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "conditional_mana_cost_context_tests.rs"]
mod cost_context_tests;
