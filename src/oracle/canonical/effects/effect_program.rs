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
    lower_effect_program_with_context(cursor, program, EffectSequenceLoweringContext::NoExternalX)
}

/// The same program lowering under the caller's numeric-binding policy.
pub(in crate::oracle::canonical) fn lower_effect_program_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    program: &EffectProgramNode,
    context: EffectSequenceLoweringContext,
) -> MachineResult<LoweredEffectSequence> {
    let mut steps = Vec::new();
    for step in program.steps() {
        steps.push(lower_program_step(
            cursor,
            step,
            &mut |cursor, sequence| match try_lower_local_effect_sequence_with_context(
                cursor, sequence, context,
            ) {
                EffectSequenceLoweringAttempt::Lowered(lowered) => Ok(lowered),
                EffectSequenceLoweringAttempt::UnsupportedSupportLimit(failure)
                | EffectSequenceLoweringAttempt::FailedSupport(failure)
                | EffectSequenceLoweringAttempt::FailedInvariant(failure) => Err(failure),
            },
            |failure| failure,
        )?);
    }
    concatenate_program_steps(cursor, steps)
}

/// Lowers one program step with the caller's sequence-lowering policy. Every
/// written sentence is lowered once; an outcome action owns two sentences.
pub(in crate::oracle::canonical) fn lower_program_step<E>(
    cursor: &mut OracleTokenCursor<'_>,
    step: &EffectProgramNode,
    lower: &mut impl FnMut(
        &mut OracleTokenCursor<'_>,
        &EffectSequenceNode,
    ) -> Result<LoweredEffectSequence, E>,
    program_failure: impl Fn(MachineFailure) -> E,
) -> Result<LoweredEffectSequence, E> {
    if let EffectProgramNode::Outcome {
        permission,
        producer: OutcomeProducer::Action(action),
        outcome,
        body,
        otherwise,
    } = step
    {
        let action = lower(cursor, action.inner_sequence())?;
        let body = lower(cursor, body.inner_sequence())?;
        let otherwise = match otherwise {
            Some(otherwise) => Some(lower(cursor, otherwise.inner_sequence())?),
            None => None,
        };
        return lower_action_outcome(
            cursor,
            permission.as_ref(),
            action,
            *outcome,
            body,
            otherwise,
        )
        .map_err(program_failure);
    }
    if let EffectProgramNode::Outcome {
        permission,
        producer: OutcomeProducer::Cost(cost),
        outcome,
        body,
        otherwise: Some(otherwise),
    } = step
    {
        let body = lower(cursor, body.inner_sequence())?;
        let otherwise = lower(cursor, otherwise.inner_sequence())?;
        return lower_optional_cost_outcome(
            cursor,
            permission.as_ref(),
            cost,
            *outcome,
            body,
            Some(otherwise),
        )
        .map_err(program_failure);
    }
    if let EffectProgramNode::Alternatives { player, options } = step {
        let mut lowered = Vec::with_capacity(options.len());
        for (label, sequence) in options {
            lowered.push((*label, lower(cursor, sequence)?));
        }
        return lower_alternatives(cursor, player, lowered).map_err(program_failure);
    }
    let lowered = lower(cursor, step.inner_sequence())?;
    wrap_lowered_effect_program(cursor, step, lowered).map_err(program_failure)
}

/// The one instruction an outcome observes, validated against its body: the
/// runtime must measure its performance, the body must be nonempty, and the
/// two independently lowered sentences must not shadow each other's results.
fn measured_outcome_instruction(
    cursor: &mut OracleTokenCursor<'_>,
    action: &LoweredEffectSequence,
    body: &LoweredEffectSequence,
    role: OutcomeInstructionRole,
) -> MachineResult<Value> {
    let (count, measured, effects, names) = match role {
        OutcomeInstructionRole::Optional => (
            "exactly one optional instruction",
            "an optional instruction whose performance the runtime measures",
            "a nonempty optional-action outcome program",
            "distinct local results across the optional action and its outcome",
        ),
        OutcomeInstructionRole::Mandatory => (
            "exactly one outcome-producing instruction",
            "an instruction whose performance the runtime measures",
            "a nonempty outcome program",
            "distinct local results across the instruction and its outcome",
        ),
    };
    let [instruction] = action.effects.as_slice() else {
        return Err(cursor.fail(count));
    };
    if !optional_action_measured(instruction) {
        return Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::MeasuredPerformance,
            measured,
        ));
    }
    cursor.enter(match role {
        OutcomeInstructionRole::Optional => "lower.program.optional-action.effects",
        OutcomeInstructionRole::Mandatory => "lower.program.reflexive-action.effects",
    });
    cursor.require(!body.effects.is_empty(), effects)?;
    let instruction_names = local_result_names(instruction);
    cursor.require(
        body.effects
            .iter()
            .all(|effect| local_result_names(effect).is_disjoint(&instruction_names)),
        names,
    )?;
    Ok(instruction.clone())
}

/// `optionalEffects{chooseOneEffect}`: the performer may perform one of the
/// coordinated actions. Each option was lowered from the same written object,
/// so their declarations must be the same one declaration.
fn lower_alternatives(
    cursor: &mut OracleTokenCursor<'_>,
    player: &LocalPlayerReference,
    options: Vec<(&'static str, LoweredEffectSequence)>,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.alternatives.actor");
    let player = lower_optional_program_player(cursor, player)?;
    cursor.enter("lower.program.alternatives.effects");
    cursor.require(
        options.len() >= 2 && options.iter().all(|(_, option)| !option.effects.is_empty()),
        "two nonempty coordinated alternatives",
    )?;
    cursor.enter("lower.program.alternatives.declarations");
    let decisions = options[0].1.decisions.clone();
    cursor.require(
        options
            .iter()
            .all(|(_, option)| option.decisions == decisions),
        "one declaration of the object every alternative acts on",
    )?;
    let options = options
        .into_iter()
        .map(|(label, option)| json!({ "label": label, "effects": option.effects }))
        .collect::<Vec<_>>();
    Ok(LoweredEffectSequence {
        decisions,
        effects: vec![json!({
            "kind": "optionalEffects",
            "player": player.clone(),
            "effects": [{
                "kind": "chooseOneEffect",
                "player": player,
                "options": options,
            }],
        })],
    })
}

#[derive(Clone, Copy)]
enum OutcomeInstructionRole {
    Optional,
    Mandatory,
}

/// An instruction producer and its outcome body. With a permission the
/// performer chooses (`optionalAction`); without one the mandatory
/// instruction runs and its measured performance gates the body
/// (`performAction`).
fn lower_action_outcome(
    cursor: &mut OracleTokenCursor<'_>,
    permission: Option<&LocalPlayerReference>,
    action: LoweredEffectSequence,
    outcome: PerformanceOutcome,
    body: LoweredEffectSequence,
    otherwise: Option<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    match permission {
        Some(player) => lower_optional_action(cursor, player, action, outcome, body, otherwise),
        None => {
            cursor.enter("lower.program.reflexive-action.complement");
            cursor.require(
                otherwise.is_none(),
                "one outcome of a mandatory instruction (its complement has no witness)",
            )?;
            lower_performed_action(cursor, action, outcome, body)
        }
    }
}

/// The two complementary outcome programs in `onPerformed`/`onDeclined`
/// order: `body` resolves for `outcome`, `otherwise` for its complement.
fn complementary_effects(
    outcome: PerformanceOutcome,
    body: Vec<Value>,
    otherwise: Option<Vec<Value>>,
) -> (Vec<Value>, Option<Vec<Value>>) {
    match outcome {
        PerformanceOutcome::Declined => (otherwise.unwrap_or_default(), Some(body)),
        PerformanceOutcome::Performed | PerformanceOutcome::Reflexive => (body, otherwise),
    }
}

/// `performAction{action, onPerformed}`: the mandatory instruction runs, and
/// only its measured performance runs the outcome. `If you do` keeps the
/// body's declarations on the enclosing ability (CR 608.2c); a reflexive
/// ability declares its own targets when it is put on the stack (CR 603.12).
fn lower_performed_action(
    cursor: &mut OracleTokenCursor<'_>,
    action: LoweredEffectSequence,
    outcome: PerformanceOutcome,
    body: LoweredEffectSequence,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.reflexive-action.action");
    let instruction =
        measured_outcome_instruction(cursor, &action, &body, OutcomeInstructionRole::Mandatory)?;
    let mut decisions = action.decisions;
    let performed = match outcome {
        PerformanceOutcome::Performed => {
            decisions.extend(body.decisions);
            body.effects
        }
        PerformanceOutcome::Reflexive => vec![reflexive_trigger(&controller(), body)],
        PerformanceOutcome::Declined => {
            return Err(cursor.fail(
                "an `If you do` or `When you do` outcome; a mandatory instruction has no declined branch",
            ));
        }
    };
    cursor.enter("lower.program.reflexive-action.declarations");
    require_distinct_declarations(
        cursor,
        &decisions,
        "distinct declaration ids across the instruction and its outcome",
    )?;
    Ok(LoweredEffectSequence {
        decisions,
        effects: vec![json!({
            "kind": "performAction",
            "action": instruction,
            "onPerformed": performed,
        })],
    })
}

fn require_distinct_declarations(
    cursor: &mut OracleTokenCursor<'_>,
    decisions: &[Value],
    expected: &str,
) -> MachineResult<()> {
    let mut ids = std::collections::BTreeSet::new();
    for decision in decisions {
        if let Some(id) = decision["id"].as_str() {
            cursor.require(ids.insert(id.to_string()), expected)?;
        }
    }
    Ok(())
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
        // Every card held when the instruction began was discarded.
        Some("discardHand") => single_player(&instruction["player"]),
        // The full count left the player's graveyard (CR 701.13a).
        Some("exileCards") => single_player(&instruction["from"]["player"]),
        // At least the stated number of counters left the one object.
        Some("removeCounters") => {
            instruction["permanent"].is_object() && instruction["counter"].is_string()
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
    outcome: PerformanceOutcome,
    body: LoweredEffectSequence,
    otherwise: Option<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.optional-action.actor");
    let player = lower_optional_program_player(cursor, player)?;
    cursor.enter("lower.program.optional-action.action");
    // Both sentences were lowered independently; a runtime binding or
    // decision produced by the instruction must not be shadowed by its outcome.
    let instruction =
        measured_outcome_instruction(cursor, &action, &body, OutcomeInstructionRole::Optional)?;
    if let Some(otherwise) = otherwise.as_ref() {
        measured_outcome_instruction(cursor, &action, otherwise, OutcomeInstructionRole::Optional)?;
    }
    let mut decisions = action.decisions;
    let (performed, declined) = match outcome {
        PerformanceOutcome::Performed | PerformanceOutcome::Declined => {
            decisions.extend(body.decisions);
            let otherwise = otherwise.map(|otherwise| {
                decisions.extend(otherwise.decisions);
                otherwise.effects
            });
            complementary_effects(outcome, body.effects, otherwise)
        }
        PerformanceOutcome::Reflexive => (vec![reflexive_trigger(&player, body)], None),
    };
    cursor.enter("lower.program.optional-action.declarations");
    require_distinct_declarations(
        cursor,
        &decisions,
        "distinct declaration ids across the optional action and its outcome",
    )?;
    let mut effect = json!({
        "kind": "optionalAction",
        "player": player,
        "action": instruction,
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
        EffectProgramNode::Alternatives { .. } => {
            cursor.enter("lower.program.alternatives");
            Err(cursor.fail("coordinated alternatives lowered with their options"))
        }
        EffectProgramNode::Outcome {
            producer: OutcomeProducer::Action(_),
            ..
        } => {
            cursor.enter("lower.program.optional-action");
            Err(cursor.fail("an outcome action lowered with its action sentence"))
        }
        EffectProgramNode::Outcome {
            permission,
            producer: OutcomeProducer::Cost(cost),
            outcome,
            body,
            otherwise,
        } => {
            cursor.enter("lower.program.optional-cost.complement");
            cursor.require(
                otherwise.is_none(),
                "a complementary outcome lowered with its own sentence",
            )?;
            let lowered = wrap_lowered_effect_program(cursor, body, lowered)?;
            lower_optional_cost_outcome(cursor, permission.as_ref(), cost, *outcome, lowered, None)
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

/// `optionalPayCostPerformEffects`: the payer may pay the resolution cost;
/// the outcome program (and its complement, when written) runs on whether
/// the cost was actually paid.
fn lower_optional_cost_outcome(
    cursor: &mut OracleTokenCursor<'_>,
    permission: Option<&LocalPlayerReference>,
    cost: &ResolutionCostNode,
    outcome: PerformanceOutcome,
    lowered: LoweredEffectSequence,
    otherwise: Option<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.optional-cost.actor");
    let player =
        permission.ok_or_else(|| cursor.fail("a permission that offers the resolution cost"))?;
    let player = lower_optional_program_player(cursor, player)?;
    cursor.enter("lower.program.optional-cost.effects");
    cursor.require(
        !lowered.effects.is_empty()
            && otherwise
                .as_ref()
                .is_none_or(|otherwise| !otherwise.effects.is_empty()),
        "a nonempty optional-cost outcome program",
    )?;
    let cost = lower_resolution_cost(cost);
    let (decisions, effects, decline) = match outcome {
        // Targets of an ordinary outcome belong to the enclosing ability
        // and are declared when it is put on the stack.
        PerformanceOutcome::Performed | PerformanceOutcome::Declined => {
            let mut decisions = lowered.decisions;
            let otherwise = otherwise.map(|otherwise| {
                decisions.extend(otherwise.decisions);
                otherwise.effects
            });
            let (effects, decline) = complementary_effects(outcome, lowered.effects, otherwise);
            (decisions, effects, decline.unwrap_or_default())
        }
        // CR 603.12: the reflexive ability chooses its own targets when it
        // is put on the stack after the cost is paid.
        PerformanceOutcome::Reflexive => {
            (vec![], vec![reflexive_trigger(&player, lowered)], vec![])
        }
    };
    cursor.enter("lower.program.optional-cost.declarations");
    require_distinct_declarations(
        cursor,
        &decisions,
        "distinct declaration ids across the outcome and its complement",
    )?;
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

fn lower_optional_program_player(
    cursor: &mut OracleTokenCursor<'_>,
    player: &LocalPlayerReference,
) -> MachineResult<Value> {
    match player {
        LocalPlayerReference::Controller => Ok(controller()),
        // Another player the binding environment names chooses (the event's
        // player, an event object's controller); the runtime asks that player.
        LocalPlayerReference::Bound(reference) => {
            bound_player_reference_value(*reference, &[], &[])
                .ok_or_else(|| cursor.fail("a player named without a declaration of this program"))
        }
        LocalPlayerReference::ControllerOf(_) | LocalPlayerReference::ControllerOfQualified(_) => {
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
        ResolutionCostNode::Payment(ResourcePaymentNode::PlayerCounters(counters)) => {
            lower_resource_player_counter_payment(counters, None)
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
        ResolutionCostNode::ReturnToHand {
            object,
            exclude_source,
        } => {
            let mut value = json!({
                "kind": "returnPermanentToOwnersHand",
                "where": lower_permanent_object_predicate(&object.predicate),
            });
            if *exclude_source {
                value["excludeSource"] = json!(true);
            }
            value
        }
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
