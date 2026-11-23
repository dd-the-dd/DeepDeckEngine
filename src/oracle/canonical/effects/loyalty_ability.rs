use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_loyalty_ability_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let magnitude_index = usize::from(
        tokens
            .first()
            .is_some_and(|token| matches!(token.raw.as_str(), "+" | "-" | "−" | "âˆ’")),
    );
    tokens
        .get(magnitude_index)
        .is_some_and(|token| token.token_type == "quantity")
        && tokens
            .get(magnitude_index + 1)
            .is_some_and(|token| token.raw == ":")
}

fn lower_loyalty_cost(cost: LoyaltyCostNode) -> Value {
    json!({
        "kind": "payLoyalty",
        "object": self_ref(),
        "amount": integer(cost.delta),
    })
}

fn lower_bound_condition(condition: &BoundObjectConditionNode) -> Value {
    debug_assert_eq!(
        condition.object,
        TargetBinding {
            effect_index: 0,
            slot_index: 0,
        }
    );
    json!({
        "kind": "objectMatchesFilter",
        "object": chosen_target(LOYALTY_GRAVEYARD_TARGET_ID),
        "where": lower_card_object_predicate(&condition.predicate),
    })
}

fn lower_atomic_effect(effect: &LoyaltyAtomicEffectNode) -> MachineResult<Value> {
    match effect {
        LoyaltyAtomicEffectNode::InstallTemporaryTrigger(trigger) => {
            Ok(lower_temporary_protected_attack_trigger(trigger))
        }
        LoyaltyAtomicEffectNode::ReturnTargetCard(effect) => {
            Ok(lower_return_target_card_effect(effect))
        }
        LoyaltyAtomicEffectNode::AddMana(effect) => Ok(lower_add_mana_effect(effect)),
        LoyaltyAtomicEffectNode::DrawCards(effect) => {
            let count = lower_numeric_expression(&effect.count).ok_or_else(|| MachineFailure {
                automaton: "loyalty-ability",
                state: "lower.draw-count",
                token_index: 0,
                span_start: 0,
                span_end: 0,
                expected: "a target-independent draw count".to_string(),
                actual: None,
                trace: Vec::new(),
                owner: None,
            })?;
            Ok(json!({
                "kind": "drawCards",
                "player": controller(),
                "count": count,
            }))
        }
        LoyaltyAtomicEffectNode::CreateEmblem(effect) => Ok(lower_create_emblem_effect(effect)),
        LoyaltyAtomicEffectNode::Conditional(conditional) => Ok(json!({
            "kind": "conditionalEffect",
            "condition": lower_bound_condition(&conditional.condition),
            "then": conditional
                .then_effects
                .iter()
                .map(lower_atomic_effect)
                .collect::<MachineResult<Vec<_>>>()?,
            "else": [],
        })),
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_loyalty_ability(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("loyalty-ability", input.tokens);
    let ability = match recognize_loyalty_ability(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        Ok(ability) => ability,
        // The specific shapes above keep their IR; every other body composes
        // from the shared activated effect program.
        Err(specific) => {
            return match parse_loyalty_effect_program(input) {
                Ok(draft) => Ok(draft),
                Err(generic) if generic.token_index >= specific.token_index => Err(generic),
                Err(_) => Err(specific),
            };
        }
    };
    let mut decisions = Vec::new();
    for effect in &ability.effects {
        if let LoyaltyAtomicEffectNode::ReturnTargetCard(effect) = effect {
            decisions.push(lower_return_target_card_declaration(effect));
        }
    }
    let effects = ability
        .effects
        .iter()
        .map(lower_atomic_effect)
        .collect::<MachineResult<Vec<_>>>()?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": [lower_loyalty_cost(ability.cost)],
        "activationCondition": { "kind": "sorceryTiming" },
        "activationLimit": { "kind": "oncePerTurn", "id": "loyaltyAbility" },
        "effects": effects,
    });
    if !decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": decisions,
        });
    }
    Ok(draft(
        rule,
        &[
            "Recognize one signed loyalty cost and a generic activated-ability envelope",
            "Compose duration, event, selectors, conditions and atomic effects",
            "Lower every body concept to independent primitive IR",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "loyalty_program_tests.rs"]
mod program_tests;
