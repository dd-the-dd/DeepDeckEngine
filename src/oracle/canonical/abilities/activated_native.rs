use super::super::*;
use crate::oracle::syntax::{AbilityInput, root_activation_effect_start};

/// Declares one numeric producer for the shared X value in an activation's
/// typed mana costs. Multiple X symbols share one chosen value; each symbol
/// still contributes independently to the mana payment.
pub(in crate::oracle::canonical) fn lower_activation_cost_decisions(
    costs: &ActivationCostSequenceNode,
) -> Vec<Value> {
    let mut decisions = lower_activation_numeric_cost_decisions(costs);
    decisions.extend(lower_activation_object_cost_decisions(costs));
    decisions
}

fn lower_activation_numeric_cost_decisions(costs: &ActivationCostSequenceNode) -> Vec<Value> {
    let mut decisions = Vec::new();
    if costs.costs.iter().any(
        |cost| matches!(cost, LocalActivationCostNode::PayMana(mana) if mana.contains_variable_x()),
    ) {
        decisions.push(x_value());
    }
    decisions
}

fn activation_cost_object_id(cost: &SelectedSacrificeCostNode) -> String {
    format!("activationCostPermanent{}", cost.cost_index)
}

fn lower_activation_object_cost_decisions(costs: &ActivationCostSequenceNode) -> Vec<Value> {
    costs
        .costs
        .iter()
        .filter_map(|cost| {
            let LocalActivationCostNode::SacrificeObject(cost) = cost else {
                return None;
            };
            Some(object_choice(
                &activation_cost_object_id(cost),
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": lower_permanent_object_predicate(&cost.object.predicate),
                }),
                json!({ "kind": "exactly", "value": integer(1) }),
            ))
        })
        .collect()
}

pub(in crate::oracle::canonical) fn with_activation_object_cost_source_context(
    mut rule: Value,
    costs: &ActivationCostSequenceNode,
) -> Value {
    if costs
        .costs
        .iter()
        .any(|cost| matches!(cost, LocalActivationCostNode::SacrificeObject(_)))
        && costs.source_zone_requirements().hand
    {
        rule["activationZone"] = json!("hand");
    }
    rule
}

/// Existing specialized activation parents keep their declaration shape when
/// no selected cost exists. New object choices follow every existing numeric
/// producer and its preceding dependencies, preserving the order and payload
/// of all preexisting decisions. Typed mana-X costs supply a missing producer;
/// an existing declaration with the same id is retained rather than duplicated.
pub(in crate::oracle::canonical) fn with_activation_object_cost_decisions(
    mut rule: Value,
    costs: &ActivationCostSequenceNode,
) -> Value {
    let objects = lower_activation_object_cost_decisions(costs);
    if objects.is_empty() {
        return rule;
    }
    let existing = rule["declaration"]["decisions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut decisions = lower_activation_numeric_cost_decisions(costs);
    decisions.retain(|producer| {
        !existing.iter().any(|decision| {
            decision["kind"] == producer["kind"] && decision["id"] == producer["id"]
        })
    });
    let insertion = existing
        .iter()
        .rposition(|decision| decision["kind"].as_str() == Some("chooseNumber"))
        .map_or(0, |position| position + 1);
    decisions.extend(existing[..insertion].iter().cloned());
    decisions.extend(objects);
    decisions.extend(existing[insertion..].iter().cloned());
    rule["declaration"]["kind"] = json!("castingDeclaration");
    rule["declaration"]["decisions"] = json!(decisions);
    with_activation_object_cost_source_context(rule, costs)
}

pub(in crate::oracle::canonical) fn lower_local_activation_cost(
    cost: &LocalActivationCostNode,
) -> Value {
    match cost {
        LocalActivationCostNode::PayMana(mana) => json!({
            "kind": "payMana",
            "manaCost": mana.as_oracle_cost(),
        }),
        LocalActivationCostNode::TapSelf(_) => json!({
            "kind": "tap",
            "object": self_ref(),
        }),
        LocalActivationCostNode::PayLife(payment) => json!({
            "kind": "payLife",
            "player": controller(),
            "amount": integer(payment.amount),
        }),
        LocalActivationCostNode::SacrificeSelf(sacrifice) => {
            let _confirmed_source_type = sacrifice.source_type;
            json!({
                "kind": "sacrificePermanent",
                "permanent": self_ref(),
            })
        }
        LocalActivationCostNode::SacrificeObject(cost) => json!({
            "kind": "sacrificePermanent",
            "permanent": { "kind": "chosenObject", "id": activation_cost_object_id(cost) },
        }),
        LocalActivationCostNode::DiscardSelf(_) => json!({
            "kind": "discardCard",
            "card": self_ref(),
        }),
    }
}

pub(in crate::oracle::canonical) enum CompoundZoneChangeActivationParseAttempt {
    Parsed(CanonicalRuleDraft),
    Unpromoted(MachineFailure),
    Failed(MachineFailure),
    NotClaimed,
}

/// Admits only a root targeted zone-change action, never an action word inside
/// costs, a condition, reminder or quoted granted ability. The target child
/// shares its first-slot head distinction with this parent.
fn claims_promoted_compound_zone_change_activation_family(input: &AbilityInput<'_>) -> bool {
    let tokens = input.tokens;
    if !tokens.first().is_some_and(|token| {
        token.token_type == "tapSymbol"
            || token.raw.eq_ignore_ascii_case("pay")
            || token.raw.eq_ignore_ascii_case("sacrifice")
    }) {
        return false;
    }
    let action_head = |position: usize| {
        let Some(action) = tokens.get(position) else {
            return false;
        };
        if !["destroy", "exile", "return"]
            .iter()
            .any(|word| action.raw.eq_ignore_ascii_case(word))
        {
            return false;
        }
        let cursor = OracleTokenCursor::new("compound-zone-change-head", &tokens[position + 1..]);
        starts_explicit_target_slot(&cursor)
    };
    if let Some(position) = root_activation_effect_start(input) {
        return action_head(position);
    }
    // Preserve diagnostics for a missing colon after an otherwise promoted
    // cost/action envelope. Quoted and parenthesized nested instructions are
    // excluded; the actual cost child still diagnoses a malformed cost.
    let mut quoted = false;
    let mut parentheses = 0_usize;
    for (position, token) in tokens.iter().enumerate().skip(1) {
        match token.raw.as_str() {
            "\"" => quoted = !quoted,
            "(" if !quoted => parentheses += 1,
            ")" if !quoted => parentheses = parentheses.saturating_sub(1),
            "." | ";" if !quoted && parentheses == 0 => return false,
            _ if !quoted && parentheses == 0 && action_head(position) => return true,
            _ => {}
        }
    }
    false
}

pub(in crate::oracle::canonical) fn try_parse_promoted_compound_zone_change_activation(
    input: &AbilityInput<'_>,
) -> CompoundZoneChangeActivationParseAttempt {
    if !claims_promoted_compound_zone_change_activation_family(input) {
        return CompoundZoneChangeActivationParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("compound-zone-change-activation", input.tokens);
    let ability = match try_recognize_compound_activated_zone_change(&mut cursor) {
        CompoundActivatedZoneChangeParseAttempt::Parsed(ability) => ability,
        CompoundActivatedZoneChangeParseAttempt::RejectedCandidate(failure) => {
            return CompoundZoneChangeActivationParseAttempt::Unpromoted(failure);
        }
        CompoundActivatedZoneChangeParseAttempt::FailedCommitted(failure) => {
            return CompoundZoneChangeActivationParseAttempt::Failed(failure);
        }
    };
    match lower_compound_zone_change_activation(&mut cursor, ability) {
        Ok(draft) => CompoundZoneChangeActivationParseAttempt::Parsed(draft),
        Err(failure) => CompoundZoneChangeActivationParseAttempt::Failed(failure),
    }
}

fn lower_compound_zone_change_activation(
    cursor: &mut OracleTokenCursor<'_>,
    ability: CompoundActivatedZoneChangeNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("activation.compound.validation");
    cursor.require(
        !ability.costs.costs.is_empty(),
        "at least one independent activation cost",
    )?;

    let costs = ability
        .costs
        .costs
        .iter()
        .map(lower_local_activation_cost)
        .collect::<Vec<_>>();
    let (target_id, candidates, minimum, maximum) = lower_single_target(&ability.effect.target);
    let effect = lower_targeted_zone_change_effect(ability.effect.action, chosen_target(target_id));
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        with_activation_object_cost_decisions(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        target_id,
                        candidates,
                        minimum,
                        maximum,
                    )],
                },
                "effects": [effect],
            }),
            &ability.costs,
        ),
        &[
            "Recognize a zone-change activation envelope from the shared token stream",
            "Compose activation costs as independent typed children",
            "Delegate the target predicate to the shared target-list and selector automata",
            "Lower the zone change to one primitive operation",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "activation_cost_decisions_tests.rs"]
mod activation_cost_decisions_tests;

#[cfg(test)]
#[path = "compound_zone_claim_tests.rs"]
mod compound_zone_claim_tests;
