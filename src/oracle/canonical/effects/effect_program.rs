//! Lower program operators after their ordinary child actions and targets.
use super::super::*;

#[cfg(test)]
#[path = "optional_cost_steps_tests.rs"]
mod optional_cost_steps_tests;

#[cfg(test)]
#[path = "optional_action_program_tests.rs"]
mod optional_action_program_tests;

#[cfg(test)]
#[path = "reflexive_action_program_tests.rs"]
mod reflexive_action_program_tests;

pub(in crate::oracle::canonical) fn lower_effect_program(
    cursor: &mut OracleTokenCursor<'_>,
    program: &EffectProgramNode,
) -> MachineResult<LoweredEffectSequence> {
    let mut steps = Vec::new();
    for step in program.steps() {
        steps.push(lower_program_step(
            cursor,
            step,
            &mut |cursor, sequence| lower_local_effect_sequence(cursor, sequence),
            |failure| failure,
        )?);
    }
    concatenate_program_steps(cursor, steps)
}

/// Lowers one program step with the caller's sequence-lowering policy. Every
/// written sentence is lowered once; an optional action owns two sentences.
pub(in crate::oracle::canonical) fn lower_program_step<E>(
    cursor: &mut OracleTokenCursor<'_>,
    step: &EffectProgramNode,
    lower: &mut impl FnMut(
        &mut OracleTokenCursor<'_>,
        &EffectSequenceNode,
    ) -> Result<LoweredEffectSequence, E>,
    program_failure: impl Fn(MachineFailure) -> E,
) -> Result<LoweredEffectSequence, E> {
    if let EffectProgramNode::OptionalAction {
        player,
        action,
        outcome,
        body,
    } = step
    {
        let action = lower(cursor, action.inner_sequence())?;
        let body = lower(cursor, body.inner_sequence())?;
        return lower_optional_action(cursor, player, action, *outcome, body)
            .map_err(program_failure);
    }
    if let EffectProgramNode::ReflexiveAction { action, body } = step {
        let action = lower(cursor, action.inner_sequence())?;
        let body = lower(cursor, body.inner_sequence())?;
        return lower_reflexive_action(cursor, action, body).map_err(program_failure);
    }
    let lowered = lower(cursor, step.inner_sequence())?;
    wrap_lowered_effect_program(cursor, step, lowered).map_err(program_failure)
}

/// `performAction{action, onPerformed}`: the mandatory instruction runs, and
/// only its measured performance creates the reflexive ability (CR 603.12),
/// which declares its own targets when it is put on the stack.
fn lower_reflexive_action(
    cursor: &mut OracleTokenCursor<'_>,
    action: LoweredEffectSequence,
    body: LoweredEffectSequence,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.reflexive-action.action");
    let [instruction] = action.effects.as_slice() else {
        return Err(cursor.fail("exactly one reflexive-trigger instruction"));
    };
    cursor.require(
        optional_action_measured(instruction),
        "an instruction whose performance the runtime measures",
    )?;
    cursor.enter("lower.program.reflexive-action.effects");
    cursor.require(
        !body.effects.is_empty(),
        "a nonempty reflexive outcome program",
    )?;
    let instruction_names = local_result_names(instruction);
    cursor.require(
        body.effects
            .iter()
            .all(|effect| local_result_names(effect).is_disjoint(&instruction_names)),
        "distinct local results across the instruction and its reflexive outcome",
    )?;
    Ok(LoweredEffectSequence {
        decisions: action.decisions,
        effects: vec![json!({
            "kind": "performAction",
            "action": instruction.clone(),
            "onPerformed": [reflexive_trigger(&controller(), body)],
        })],
    })
}

/// Contract with the runtime's `optional_action_witness_supported`: the
/// primitive instructions whose actual performance the runtime measures for
/// `optionalAction` (one actor, one counted amount or referenced objects).
/// Another instruction stays an explicit failure, never `chosen = performed`.
fn optional_action_measured(instruction: &Value) -> bool {
    let single_player = |reference: &Value| {
        reference.is_object()
            && !matches!(
                reference["kind"].as_str(),
                Some("opponentsOf" | "eachPlayer" | "chosenTargets")
            )
    };
    match instruction["kind"].as_str() {
        Some("drawCards" | "mill" | "gainLife") => single_player(&instruction["player"]),
        Some("discardCards") => {
            single_player(&instruction["player"]) && instruction.get("cards").is_none()
        }
        Some("sacrificePermanents") => {
            single_player(&instruction["player"]) && instruction.get("objects").is_none()
        }
        Some("createTokens") => true,
        Some("putCounters") => matches!(
            instruction["permanent"]["kind"].as_str(),
            Some("self" | "abilitySource" | "chosenTarget")
        ),
        Some(
            "tapPermanent" | "untapPermanent" | "destroyPermanent" | "exilePermanent"
            | "sacrificePermanent",
        ) => instruction["permanent"].is_object(),
        Some("returnToOwnersHand") => instruction["object"].is_object(),
        Some("moveCards" | "exileTargetCards") => instruction["cards"].is_object(),
        _ => false,
    }
}

/// `optionalAction{player, action, onPerformed, onDeclined}`. The action's
/// targets are declared by the enclosing ability; a reflexive outcome
/// declares its own when it is put on the stack (CR 603.12).
fn lower_optional_action(
    cursor: &mut OracleTokenCursor<'_>,
    player: &LocalPlayerReference,
    action: LoweredEffectSequence,
    outcome: CostOutcomeContinuation,
    body: LoweredEffectSequence,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.optional-action.actor");
    let player = lower_optional_program_player(cursor, player)?;
    cursor.enter("lower.program.optional-action.action");
    let [instruction] = action.effects.as_slice() else {
        return Err(cursor.fail("exactly one optional instruction"));
    };
    cursor.require(
        optional_action_measured(instruction),
        "an optional instruction whose performance the runtime measures",
    )?;
    cursor.enter("lower.program.optional-action.effects");
    cursor.require(
        !body.effects.is_empty(),
        "a nonempty optional-action outcome program",
    )?;
    // Both sentences were lowered independently; a runtime binding or
    // decision produced by the instruction must not be shadowed by its outcome.
    let instruction_names = local_result_names(instruction);
    cursor.require(
        body.effects
            .iter()
            .all(|effect| local_result_names(effect).is_disjoint(&instruction_names)),
        "distinct local results across the optional action and its outcome",
    )?;
    let mut decisions = action.decisions.clone();
    let (performed, declined) = match outcome {
        CostOutcomeContinuation::IfPaid => {
            decisions.extend(body.decisions);
            (body.effects, None)
        }
        CostOutcomeContinuation::IfDeclined => {
            decisions.extend(body.decisions);
            (vec![], Some(body.effects))
        }
        CostOutcomeContinuation::WhenPaid => (vec![reflexive_trigger(&player, body)], None),
    };
    cursor.enter("lower.program.optional-action.declarations");
    let mut ids = std::collections::BTreeSet::new();
    for decision in &decisions {
        if let Some(id) = decision["id"].as_str() {
            cursor.require(
                ids.insert(id.to_string()),
                "distinct declaration ids across the optional action and its outcome",
            )?;
        }
    }
    let mut effect = json!({
        "kind": "optionalAction",
        "player": player,
        "action": instruction.clone(),
        "onPerformed": performed,
    });
    if let Some(declined) = declined {
        effect["onDeclined"] = Value::Array(declined);
    }
    Ok(LoweredEffectSequence {
        decisions,
        effects: vec![effect],
    })
}

/// Runtime binding and decision names (`bind`, `id`) produced inside an
/// effect. A declared-target reference names its declaration; it produces none.
fn local_result_names(effect: &Value) -> std::collections::BTreeSet<String> {
    fn walk(value: &Value, names: &mut std::collections::BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                let reference = matches!(
                    map.get("kind").and_then(Value::as_str),
                    Some("chosenTarget" | "chosenTargets")
                );
                for (key, child) in map {
                    if let ("bind" | "id", Value::String(name)) = (key.as_str(), child)
                        && !reference
                    {
                        names.insert(name.clone());
                    }
                    walk(child, names);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, names)),
            _ => {}
        }
    }
    let mut names = std::collections::BTreeSet::new();
    walk(effect, &mut names);
    names
}

/// CR 603.12: a reflexive ability created by a resolving ability chooses its
/// own targets when it is put on the stack.
fn reflexive_trigger(player: &Value, lowered: LoweredEffectSequence) -> Value {
    let mut ability = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
        "effects": lowered.effects,
    });
    if !lowered.decisions.is_empty() {
        ability["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }
    json!({
        "kind": "createReflexiveTrigger",
        "source": self_ref(),
        "controller": player.clone(),
        "ability": ability,
    })
}

/// Each step owns its declarations and effects; steps run in written order.
/// Declarations are ability-wide, so their ids must stay distinct.
pub(in crate::oracle::canonical) fn concatenate_program_steps(
    cursor: &mut OracleTokenCursor<'_>,
    steps: Vec<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    let mut program = LoweredEffectSequence {
        decisions: Vec::new(),
        effects: Vec::new(),
    };
    let mut ids = std::collections::BTreeSet::new();
    for step in steps {
        cursor.enter("lower.program.steps.declarations");
        for decision in &step.decisions {
            if let Some(id) = decision["id"].as_str() {
                cursor.require(
                    ids.insert(id.to_string()),
                    "distinct declaration ids across independent program steps",
                )?;
            }
        }
        program.decisions.extend(step.decisions);
        program.effects.extend(step.effects);
    }
    Ok(program)
}

/// Contextual parents lower the same sequence with their declaration policy,
/// then reuse this sole optional-control wrapper without reparsing children.
pub(in crate::oracle::canonical) fn wrap_lowered_effect_program(
    cursor: &mut OracleTokenCursor<'_>,
    program: &EffectProgramNode,
    lowered: LoweredEffectSequence,
) -> MachineResult<LoweredEffectSequence> {
    match program {
        EffectProgramNode::Sequence(_) => Ok(lowered),
        EffectProgramNode::Steps(_) => {
            cursor.enter("lower.program.steps");
            Err(cursor.fail("program steps lowered one step at a time"))
        }
        EffectProgramNode::OptionalAction { .. } | EffectProgramNode::ReflexiveAction { .. } => {
            cursor.enter("lower.program.optional-action");
            Err(cursor.fail("an outcome action lowered with its action sentence"))
        }
        EffectProgramNode::OptionalCost {
            player,
            cost,
            outcome,
            body,
        } => {
            cursor.enter("lower.program.optional-cost.actor");
            let player = lower_optional_program_player(cursor, player)?;
            let lowered = wrap_lowered_effect_program(cursor, body, lowered)?;
            cursor.enter("lower.program.optional-cost.effects");
            cursor.require(
                !lowered.effects.is_empty(),
                "a nonempty optional-cost outcome program",
            )?;
            let cost = lower_resolution_cost(cost);
            let (decisions, effects, decline) = match outcome {
                // Targets of an ordinary outcome belong to the enclosing
                // ability and are declared when it is put on the stack.
                CostOutcomeContinuation::IfPaid => (lowered.decisions, lowered.effects, vec![]),
                CostOutcomeContinuation::IfDeclined => (lowered.decisions, vec![], lowered.effects),
                // CR 603.12: the reflexive ability chooses its own targets
                // when it is put on the stack after the cost is paid.
                CostOutcomeContinuation::WhenPaid => {
                    (vec![], vec![reflexive_trigger(&player, lowered)], vec![])
                }
            };
            Ok(LoweredEffectSequence {
                decisions,
                effects: vec![json!({
                    "kind": "optionalPayCostPerformEffects",
                    "player": player,
                    "cost": cost,
                    "effects": effects,
                    "declineEffects": decline,
                })],
            })
        }
        EffectProgramNode::Optional { player, body } => {
            cursor.enter("lower.program.optional.actor");
            let player = lower_optional_program_player(cursor, player)?;
            let lowered = wrap_lowered_effect_program(cursor, body, lowered)?;
            cursor.enter("lower.program.optional.effects");
            cursor.require(
                !lowered.effects.is_empty(),
                "a nonempty optional effect program",
            )?;
            // Targets are mandatory declarations even when the action will
            // later be declined. Only execution is wrapped by permission.
            Ok(LoweredEffectSequence {
                decisions: lowered.decisions,
                effects: vec![json!({
                    "kind": "optionalEffects", "player": player,
                    "effects": lowered.effects,
                })],
            })
        }
    }
}

fn lower_optional_program_player(
    cursor: &mut OracleTokenCursor<'_>,
    player: &LocalPlayerReference,
) -> MachineResult<Value> {
    match player {
        LocalPlayerReference::Controller => Ok(controller()),
        LocalPlayerReference::ControllerOf(_)
        | LocalPlayerReference::ControllerOfQualified(_)
        | LocalPlayerReference::Bound(_) => {
            Err(cursor.fail("the ability controller for this optional program"))
        }
    }
}

/// One resolution cost program (`CostProgram` schema). The payer is the
/// enclosing program's player, so no cost carries its own payer.
pub(in crate::oracle::canonical) fn lower_resolution_cost(cost: &ResolutionCostNode) -> Value {
    match cost {
        ResolutionCostNode::Payment(ResourcePaymentNode::Mana(mana)) => {
            lower_resource_mana_payment(mana)
        }
        ResolutionCostNode::Payment(ResourcePaymentNode::Life(life)) => {
            json!({ "kind": "payLife", "amount": integer(life.amount) })
        }
        ResolutionCostNode::Sacrifice {
            object,
            exclude_source,
        } => {
            let mut value = json!({
                "kind": "sacrificePermanent",
                "where": lower_permanent_object_predicate(&object.predicate),
            });
            if *exclude_source {
                value["excludeSource"] = json!(true);
            }
            value
        }
        ResolutionCostNode::SacrificeSource => json!({
            "kind": "sacrificePermanent",
            "permanent": self_ref(),
        }),
        // The unrestricted card selector lowers to `null` (any card).
        ResolutionCostNode::DiscardCard { filter } => json!({
            "kind": "discardCard",
            "where": lower_card_object_predicate(filter),
        }),
        ResolutionCostNode::TapObjects(tap) => {
            let filter = lower_permanent_object_predicate(&tap.object.predicate);
            let mut value = if tap.count == 1 {
                json!({ "kind": "tap", "where": filter })
            } else {
                json!({ "kind": "tapPermanents", "count": integer(tap.count), "where": filter })
            };
            if tap.exclude_source {
                value["excludeSource"] = json!(true);
            }
            value
        }
        ResolutionCostNode::Blight { amount } => json!({
            "kind": "putCounters",
            "counter": "-1/-1",
            "count": integer(*amount),
            "where": card_type("Creature"),
        }),
        // `any`: one counter of a kind the payer chooses among the source's.
        ResolutionCostNode::RemoveSourceCounters { counter, count } => json!({
            "kind": "removeCounters",
            "permanent": self_ref(),
            "counter": counter.as_ref().map_or("any", |counter| counter.as_str()),
            "count": integer(*count),
        }),
        ResolutionCostNode::ExileCard {
            filter,
            zone,
            exclude_source,
        } => {
            let mut value = json!({
                "kind": "exileCard",
                "zone": match zone {
                    PaymentOriginZone::Hand => "hand",
                    PaymentOriginZone::Graveyard => "graveyard",
                },
                "where": lower_card_object_predicate(filter),
            });
            if *exclude_source {
                value["excludeSource"] = json!(true);
            }
            value
        }
        ResolutionCostNode::Sequence(parts) => json!({
            "kind": "costSequence",
            "costs": parts.iter().map(lower_resolution_cost).collect::<Vec<_>>(),
        }),
    }
}
