use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

pub(in crate::oracle::canonical) enum StaticSpellReductionParseAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_promoted_static_spell_reduction(
    input: &AbilityInput<'_>,
) -> StaticSpellReductionParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticSpellReductionParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-spell-reduction", input.tokens);
    match try_recognize_casting_cost_reduction(&mut cursor) {
        // The activated-ability subject of the same cost-adjustment frame.
        CastingReductionReadAttempt::NotClaimed => {
            let mut cursor = OracleTokenCursor::new("static-activation-cost", input.tokens);
            match try_recognize_activation_cost_modification(
                &mut cursor,
                input.name_links,
                input.name_resolution_failure,
            ) {
                ActivationCostModificationAttempt::NotClaimed => {
                    StaticSpellReductionParseAttempt::NotClaimed
                }
                ActivationCostModificationAttempt::FailedCommitted(failure) => {
                    StaticSpellReductionParseAttempt::Failed(failure)
                }
                ActivationCostModificationAttempt::Parsed(node) => {
                    match lower_activation_cost_modification(&mut cursor, node) {
                        Ok(draft) => StaticSpellReductionParseAttempt::Parsed(draft),
                        Err(failure) => StaticSpellReductionParseAttempt::Failed(failure),
                    }
                }
            }
        }
        CastingReductionReadAttempt::FailedCommitted(failure) => {
            StaticSpellReductionParseAttempt::Failed(failure)
        }
        CastingReductionReadAttempt::Parsed(node) => {
            if !matches!(&node.subject, CastingReductionSubject::SpellSet(_)) {
                return StaticSpellReductionParseAttempt::NotClaimed;
            }
            match lower_casting_cost_reduction(input, &mut cursor, node) {
                Ok(draft) => StaticSpellReductionParseAttempt::Parsed(draft),
                Err(failure) => StaticSpellReductionParseAttempt::Failed(failure),
            }
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_static_spell_reduction(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-spell-reduction", input.tokens);
    let node = recognize_static_spell_reduction(&mut cursor)?;
    lower_casting_cost_reduction(input, &mut cursor, node)
}

/// The ability classes whose activation cost the runtime determines through
/// the shared activation cost adjustment (CR 602.2b): equip, cycling,
/// loyalty and the keyword-ability classes of activated rules.
const RUNTIME_ACTIVATION_CLASSES: [&str; 7] = [
    "equip", "cycling", "loyalty", "boast", "powerUp", "exhaust", "forecast",
];

/// `ABILITIES cost DELTA less|more to activate` (CR 602.2b, 601.2f): one
/// `modifyActivationCost` modifier whose ability population keeps each
/// independent part (class, activators, sources, targeted object) and whose
/// delta is applied after the ability's own cost and before the floor.
fn lower_activation_cost_modification(
    cursor: &mut OracleTokenCursor<'_>,
    node: ActivationCostModificationNode,
) -> MachineResult<CanonicalRuleDraft> {
    let ActivationCostModificationNode {
        abilities,
        delta,
        condition,
        minimum_one_mana,
    } = node;
    cursor.enter("activation-cost.lower.class");
    if let Some(class) = abilities.class {
        cursor.require(
            RUNTIME_ACTIVATION_CLASSES.contains(&class),
            "an ability class whose activation cost the runtime determines (equip, cycling, loyalty, boast, power-up, exhaust, forecast)",
        )?;
    }
    cursor.enter("activation-cost.lower.mana-abilities");
    cursor.require(
        abilities.excludes_mana_abilities,
        "a population without mana abilities: mana abilities are activated while paying and keep their printed costs in the runtime",
    )?;
    let mut population = json!({ "excludesManaAbilities": true });
    if let Some(class) = abilities.class {
        population["class"] = json!(class);
    }
    if let Some(activators) = abilities.activators {
        population["activators"] = match activators {
            AbilityActivatorScope::Controller => controller(),
            AbilityActivatorScope::Opponents => {
                json!({ "kind": "opponentsOf", "player": controller() })
            }
        };
    }
    if let Some(sources) = &abilities.sources {
        cursor.enter("activation-cost.lower.sources");
        population["sources"] = lower_static_permanent_query(cursor, sources)?;
    }
    if let Some(target) = &abilities.target {
        cursor.enter("activation-cost.lower.target");
        population["target"] = match target {
            AbilityTargetCondition::Source => self_ref(),
            AbilityTargetCondition::Query(query) => lower_static_permanent_query(cursor, query)?,
        };
    }
    let mut modifier = json!({
        "kind": "modifyActivationCost",
        "abilities": population,
    });
    match &delta {
        ActivationCostDeltaNode::Reduce(amount) => {
            modifier["reduceGeneric"] = json!(amount);
        }
        ActivationCostDeltaNode::Increase(cost) => {
            cursor.enter("activation-cost.lower.increase");
            cost.fixed_runtime_mana_value()
                .map_err(|error| cursor.fail(error.expected()))?;
            modifier["addManaCost"] = json!(cost.as_oracle_cost());
        }
    }
    if minimum_one_mana {
        modifier["minimumOneMana"] = json!(true);
    }
    if let Some(condition) = &condition {
        let CostAdjustmentCondition::Sentence(condition) = condition else {
            return Err(cursor.fail("a sentence condition for an activation cost modification"));
        };
        cursor.enter("activation-cost.lower.condition");
        modifier["condition"] = lower_sentence_condition(
            cursor,
            condition,
            &ConditionObjectBindings {
                permanent_targets: &[],
                card_targets: &[],
                card_focus: None,
                previous_action_result: None,
            },
        )?;
    }
    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [modifier],
        }),
        &[
            "Read the activated-ability population: class, activators, sources and targeted object",
            "Read the shared mana delta and its direction",
            "Apply the delta after the ability's cost and before the one-mana floor (CR 602.2b, 601.2f)",
        ],
    ))
}
#[cfg(test)]
#[path = "static_spell_reduction_tests.rs"]
mod tests;
