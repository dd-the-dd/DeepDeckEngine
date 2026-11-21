use super::super::*;
use crate::oracle::syntax::AbilityInput;

const TARGET_ID: &str = "earthbendLand";

pub(in crate::oracle::canonical) fn earthbend_target_id(effect_index: usize) -> String {
    format!("earthbendLand{effect_index}")
}

pub(in crate::oracle::canonical) struct LoweredEarthbendAction {
    pub decision: Value,
    pub effects: Vec<Value>,
}

/// Lowers the reusable Earthbend keyword action independently from its
/// activated, triggered, or spell envelope.
pub(in crate::oracle::canonical) fn lower_earthbend_action(
    action: &EarthbendActionNode,
    target_id: &str,
    candidates: Value,
    minimum: i64,
    maximum: i64,
) -> LoweredEarthbendAction {
    let object = chosen_target(target_id);
    let duration = json!({ "kind": "permanent" });
    let quantity = lower_numeric_atom(action.quantity.clone());
    LoweredEarthbendAction {
        decision: target_decision(target_id, candidates, minimum, maximum),
        effects: vec![
            json!({
                "kind": "becomeCreature",
                "object": object.clone(),
                "addTypes": ["Creature"],
                "addSubtypes": [],
                "basePower": integer(0),
                "baseToughness": integer(0),
                "retainExistingTypes": true,
                "duration": duration.clone(),
            }),
            json!({
                "kind": "grantKeyword",
                "object": object.clone(),
                "keyword": "haste",
                "duration": duration,
            }),
            json!({
                "kind": "putCounters",
                "permanent": object.clone(),
                "counter": "+1/+1",
                "count": quantity,
            }),
            json!({
                "kind": "installDelayedZoneChangeTrigger",
                "controller": { "kind": "abilityController" },
                "trackedObject": object,
                "destinations": ["graveyard", "exile"],
                "effects": [{
                    "kind": "moveTriggeringCard",
                    "from": { "kind": "triggeringDestination" },
                    "to": "battlefield",
                    "controller": { "kind": "abilityController" },
                    "tapped": true,
                }],
            }),
            json!({
                "kind": "emitAbilitySignal",
                "abilityId": "earthbend",
                "player": { "kind": "abilityController" },
            }),
        ],
    }
}

pub(in crate::oracle::canonical) fn claims_promoted_earthbend_activation_family(
    input: &AbilityInput<'_>,
) -> bool {
    claims_earthbend_activation(input)
}

pub(in crate::oracle::canonical) fn parse_promoted_earthbend_activation(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("earthbend-activation", input.tokens);
    let node = recognize_earthbend_activation(&mut cursor)?;
    cursor.enter("earthbend.activation.target-contract");

    let (candidates, minimum, maximum) = if let Some(reminder) = &node.reminder {
        let slot = reminder
            .target
            .slots
            .first()
            .ok_or_else(|| cursor.fail("one controlled land target"))?;
        let (_, candidates) = lower_target_selector_candidates(&slot.selector)
            .ok_or_else(|| cursor.fail("a lowerable controlled land target"))?;
        (
            candidates,
            i64::from(slot.cardinality.minimum),
            i64::from(slot.cardinality.maximum),
        )
    } else {
        (
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Land"),
            }),
            1,
            1,
        )
    };

    let uses_x = numeric_atom_uses_x(&node.quantity);
    let costs = node
        .costs
        .costs
        .iter()
        .flat_map(lower_local_activation_costs)
        .collect::<Vec<_>>();
    let lowered = lower_earthbend_action(
        &EarthbendActionNode {
            quantity: node.quantity,
        },
        TARGET_ID,
        candidates,
        minimum,
        maximum,
    );
    let mut decisions = Vec::new();
    if uses_x {
        decisions.push(x_value());
    }
    decisions.push(lowered.decision);

    Ok(draft(
        with_activation_object_cost_decisions(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                },
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": lowered.effects,
            }),
            &node.costs,
        ),
        &[
            "Compose mana and tap costs through the shared activation-cost automaton",
            "Validate sorcery timing and the optional typed Earthbend reminder",
            "Declare one controlled land through the shared target selector",
            "Lower animation, haste, counters, delayed return, and signal as independent primitives",
        ],
    ))
}

#[cfg(test)]
#[path = "earthbend_lowering_tests.rs"]
mod tests;
