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
        scope,
        result,
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
            *scope,
            result.as_ref(),
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
        ..
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
    if let EffectProgramNode::Offer {
        players,
        excluding_controller,
        producer,
        performed,
        declined,
    } = step
    {
        let action = lower(cursor, producer)?;
        let performed = match performed {
            Some(body) => Some(lower(cursor, body)?),
            None => None,
        };
        let declined = match declined {
            Some(body) => Some(lower(cursor, body)?),
            None => None,
        };
        return lower_offered_action(
            cursor,
            *players,
            *excluding_controller,
            action,
            [performed, declined],
        )
        .map_err(program_failure);
    }
    if let EffectProgramNode::CreatedReplacement { replacement, .. } = step {
        return lower_created_replacement_step(cursor, replacement).map_err(program_failure);
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

/// The instruction an outcome observes (CR 608.2c, 603.12): one measured
/// primitive (`action`) or an instruction group (`actions`): the shared
/// effect sequence of the producing sentence, whose choices and actions the
/// runtime measures together (`reveal a Dinosaur card from your hand`,
/// `exile two cards from your graveyard`, `discard your hand and draw two
/// cards`).
#[derive(Clone, Debug)]
enum MeasuredInstruction {
    Single(Value),
    Group(Vec<Value>),
}

impl MeasuredInstruction {
    fn effects(&self) -> &[Value] {
        match self {
            Self::Single(instruction) => std::slice::from_ref(instruction),
            Self::Group(members) => members,
        }
    }

    /// Installs the instruction on its outcome effect.
    fn install(self, effect: &mut Value) {
        match self {
            Self::Single(instruction) => effect["action"] = instruction,
            Self::Group(members) => effect["actions"] = Value::Array(members),
        }
    }
}

/// The role one lowered member plays in a measured instruction group; the
/// contract mirrors the runtime's `measured_group_supported`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupMemberRole {
    /// A choice a later member acts on (`chooseCards`, `choosePermanents`):
    /// it must choose the number the instruction asks for.
    Selection,
    /// A bound value or an always-performed bookkeeping step.
    Neutral,
    /// An action whose performance the runtime measures.
    Measured,
}

fn group_member_role(member: &Value) -> Option<GroupMemberRole> {
    match member["kind"].as_str() {
        Some("chooseCards" | "choosePermanents") if member["id"].is_string() => {
            Some(GroupMemberRole::Selection)
        }
        Some("bind") if member["id"].is_string() => Some(GroupMemberRole::Neutral),
        Some("shuffleZone" | "shuffleLibrariesHolding" | "recordLibrarySearch") => {
            Some(GroupMemberRole::Neutral)
        }
        Some("revealCards") => member["cards"]
            .is_object()
            .then_some(GroupMemberRole::Measured),
        Some("discardCards") if member.get("cards").is_some() => member["cards"]
            .is_object()
            .then_some(GroupMemberRole::Measured),
        Some("sacrificePermanents") if member.get("objects").is_some() => member["objects"]
            .is_object()
            .then_some(GroupMemberRole::Measured),
        _ => optional_action_measured(member).then_some(GroupMemberRole::Measured),
    }
}

/// The measured instruction of a lowered producing sentence: one measured
/// primitive, a disjunction of measured groups, or a group of members that
/// all have a role and at least one measured action. `None` otherwise.
fn measured_instruction_of(effects: &[Value]) -> Option<MeasuredInstruction> {
    if let [instruction] = effects
        && (optional_action_measured(instruction) || measured_disjunction(instruction))
    {
        return Some(MeasuredInstruction::Single(instruction.clone()));
    }
    let roles = effects
        .iter()
        .map(group_member_role)
        .collect::<Option<Vec<_>>>()?;
    roles
        .contains(&GroupMemberRole::Measured)
        .then(|| MeasuredInstruction::Group(effects.to_vec()))
}

/// `sacrifice an artifact or discard a card` (`chooseOneEffect`): the
/// performer chooses one option; each option is its own measured group.
fn measured_disjunction(instruction: &Value) -> bool {
    instruction["kind"] == "chooseOneEffect"
        && instruction["options"].as_array().is_some_and(|options| {
            options.len() >= 2
                && options.iter().all(|option| {
                    option["effects"]
                        .as_array()
                        .is_some_and(|effects| measured_instruction_of(effects).is_some())
                })
        })
}

/// The instruction an outcome observes, validated against its body: the
/// runtime must measure its performance, the body must be nonempty, and the
/// two independently lowered sentences must not shadow each other's results.
fn measured_outcome_instruction(
    cursor: &mut OracleTokenCursor<'_>,
    action: &[Value],
    body: &LoweredEffectSequence,
    role: OutcomeInstructionRole,
) -> MachineResult<MeasuredInstruction> {
    let (count, measured, effects, names) = match role {
        OutcomeInstructionRole::Optional => (
            "an optional instruction",
            "an optional instruction whose performance the runtime measures",
            "a nonempty optional-action outcome program",
            "distinct local results across the optional action and its outcome",
        ),
        OutcomeInstructionRole::Mandatory => (
            "an outcome-producing instruction",
            "an instruction whose performance the runtime measures",
            "a nonempty outcome program",
            "distinct local results across the instruction and its outcome",
        ),
    };
    cursor.require(!action.is_empty(), count)?;
    let Some(instruction) = measured_instruction_of(action) else {
        return Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::MeasuredPerformance,
            measured,
        ));
    };
    cursor.enter(match role {
        OutcomeInstructionRole::Optional => "lower.program.optional-action.effects",
        OutcomeInstructionRole::Mandatory => "lower.program.reflexive-action.effects",
    });
    cursor.require(!body.effects.is_empty(), effects)?;
    let instruction_names = instruction
        .effects()
        .iter()
        .flat_map(local_result_names)
        .collect::<std::collections::BTreeSet<_>>();
    cursor.require(
        body.effects
            .iter()
            .all(|effect| local_result_names(effect).is_disjoint(&instruction_names)),
        names,
    )?;
    Ok(instruction)
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
/// (`performAction`). A producer that extends over earlier sentences (whose
/// antecedents its own sentence reads) runs them first, unmeasured: the
/// outcome observes only its final instruction.
fn lower_action_outcome(
    cursor: &mut OracleTokenCursor<'_>,
    permission: Option<&LocalPlayerReference>,
    scope: ProducerScope,
    result: Option<&ReflexiveResultNode>,
    action: LoweredEffectSequence,
    outcome: PerformanceOutcome,
    body: LoweredEffectSequence,
    otherwise: Option<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    let LoweredEffectSequence {
        decisions: mut declarations,
        mut effects,
    } = action;
    let prefix = match scope {
        ProducerScope::Sentence => Vec::new(),
        ProducerScope::FinalInstruction => {
            cursor.enter("lower.program.outcome.final-instruction");
            // Only a node that wraps its whole sentence (a sentence condition
            // or the sentence's own permission) is known to be that
            // sentence's instruction; any other final effect may be one of
            // several actions of the final sentence.
            let last = effects
                .pop()
                .filter(|last| {
                    matches!(
                        last["kind"].as_str(),
                        Some("conditional" | "optionalEffects")
                    )
                })
                .ok_or_else(|| {
                    cursor
                        .fail("a final conditional or permission instruction the outcome observes")
                })?;
            std::mem::replace(&mut effects, vec![last])
        }
    };
    let player = match permission {
        Some(player) => {
            cursor.enter("lower.program.optional-action.actor");
            Some(lower_optional_program_player(cursor, player)?)
        }
        None => None,
    };
    let optional = player.is_some();
    let lowered = lower_gated_outcome(cursor, player, effects, result, outcome, body, otherwise)?;
    declarations.extend(lowered.decisions);
    cursor.enter(if optional {
        "lower.program.optional-action.declarations"
    } else {
        "lower.program.reflexive-action.declarations"
    });
    require_distinct_declarations(
        cursor,
        &declarations,
        "distinct declaration ids across the instruction and its outcome",
    )?;
    let mut program = prefix;
    program.extend(lowered.effects);
    Ok(LoweredEffectSequence {
        decisions: declarations,
        effects: program,
    })
}

/// The producer's lowered instruction and the outcome it gates. A producing
/// sentence gated by its own condition (`If it's a land card, you may put it
/// onto the battlefield.`, `Then if you control four or more Treasures,
/// sacrifice this Saga.`) performs nothing when the condition is false, so
/// that branch runs the declined outcome (CR 608.2c). A permission written
/// inside the producing sentence (`If COND, you may ACTION.`) is the
/// optional instruction's own.
fn lower_gated_outcome(
    cursor: &mut OracleTokenCursor<'_>,
    player: Option<Value>,
    effects: Vec<Value>,
    result: Option<&ReflexiveResultNode>,
    outcome: PerformanceOutcome,
    body: LoweredEffectSequence,
    otherwise: Option<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    if player.is_none()
        && let [gate] = effects.as_slice()
        && gate["kind"] == "conditional"
        && gate["else"].as_array().is_some_and(Vec::is_empty)
        && let Some(then) = gate["then"].as_array().filter(|then| !then.is_empty())
    {
        cursor.enter("lower.program.outcome.gated-producer");
        let when_false = match outcome {
            PerformanceOutcome::Declined => Some(body.effects.clone()),
            PerformanceOutcome::Performed | PerformanceOutcome::Reflexive => otherwise
                .as_ref()
                .map(|otherwise| otherwise.effects.clone()),
        };
        let mut gate = gate.clone();
        let inner =
            lower_gated_outcome(cursor, None, then.clone(), result, outcome, body, otherwise)?;
        gate["then"] = Value::Array(inner.effects);
        gate["else"] = Value::Array(when_false.unwrap_or_default());
        return Ok(LoweredEffectSequence {
            decisions: inner.decisions,
            effects: vec![gate],
        });
    }
    // One player decides; a population's permission (`each opponent may`)
    // has no single outcome and stays a measured-performance failure.
    if player.is_none()
        && let [optional] = effects.as_slice()
        && optional["kind"] == "optionalEffects"
        && optional.get("revealSource").is_none()
        && optional["player"].is_object()
        && !matches!(
            optional["player"]["kind"].as_str(),
            Some("opponentsOf" | "eachPlayer" | "chosenTargets" | "eachOpponent")
        )
        && let Some(inner) = optional["effects"].as_array()
    {
        cursor.enter("lower.program.outcome.inner-permission");
        if let Some(result) = result {
            return lower_reflexive_result(
                cursor,
                Some(optional["player"].clone()),
                inner.clone(),
                result,
                body,
            );
        }
        return lower_optional_action(
            cursor,
            optional["player"].clone(),
            inner.clone(),
            outcome,
            body,
            otherwise,
        );
    }
    if let Some(result) = result {
        return lower_reflexive_result(cursor, player, effects, result, body);
    }
    match player {
        Some(player) => lower_optional_action(cursor, player, effects, outcome, body, otherwise),
        None => lower_performed_action(cursor, effects, outcome, body, otherwise),
    }
}

/// `INSTRUCTION. When you VERB [one or more] NOUN this way, BODY.` (CR
/// 603.12): the instruction runs as written (it is not gated on its own
/// performance), the action of the restated verb records the objects it
/// affected, and a reflexive ability is created when at least one of them
/// satisfies the noun. The reflexive ability declares its own targets when
/// it is put on the stack. With a permission, the whole block is the
/// performer's optional instruction.
fn lower_reflexive_result(
    cursor: &mut OracleTokenCursor<'_>,
    player: Option<Value>,
    mut effects: Vec<Value>,
    result: &ReflexiveResultNode,
    body: LoweredEffectSequence,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.reflexive-result.producer");
    // The nearest action of the instruction that runs the restated verb.
    let position = effects
        .iter()
        .rposition(|member| result.verb.records_result(member))
        .ok_or_else(|| {
            cursor.fail("an action of the instruction that the reflexive event restates")
        })?;
    cursor.require(
        result.one_or_more || affects_one_object(&effects[position]),
        "a single affected object for a singular reflexive event (`a`/`an`), or `one or more`",
    )?;
    let objects = action_result_objects(&mut effects[position], position)
        .map_err(|expected| cursor.fail(expected))?;
    cursor.enter("lower.program.reflexive-result.objects");
    let restated = if result.predicate == ObjectPredicate::ObjectKind(ObjectKind::Card) {
        objects
    } else if result.verb.preserves_card_characteristics()
        && result.predicate.is_in_domain(ObjectKind::Card)
    {
        json!({
            "kind": "filterObjects",
            "objects": objects,
            "where": lower_card_object_predicate(&result.predicate),
        })
    } else {
        return Err(cursor.fail("a restated card noun the affected cards keep readable"));
    };
    cursor.enter("lower.program.reflexive-result.body");
    cursor.require(!body.effects.is_empty(), "a nonempty reflexive ability")?;
    let producer_names = effects
        .iter()
        .flat_map(local_result_names)
        .collect::<std::collections::BTreeSet<_>>();
    cursor.require(
        body.effects
            .iter()
            .all(|effect| local_result_names(effect).is_disjoint(&producer_names)),
        "distinct local results across the instruction and its reflexive ability",
    )?;
    let reflexive = reflexive_trigger(&player.clone().unwrap_or_else(controller), body);
    effects.push(json!({
        "kind": "conditional",
        "condition": {
            "kind": "compare",
            "left": { "kind": "countObjects", "objects": restated },
            "operator": ">=",
            "right": { "kind": "integer", "value": 1 },
        },
        "then": [reflexive],
        "else": [],
    }));
    let effects = match player {
        Some(player) => vec![json!({
            "kind": "optionalEffects",
            "player": player,
            "effects": effects,
        })],
        None => effects,
    };
    Ok(LoweredEffectSequence {
        decisions: Vec::new(),
        effects,
    })
}

/// True when the action affects at most one object: a literal count of one,
/// or one declared target.
fn affects_one_object(member: &Value) -> bool {
    let one = json!({ "kind": "integer", "value": 1 });
    match member["kind"].as_str() {
        Some("discardCards" | "mill" | "sacrificePermanents") => {
            member.get("cards").is_none()
                && member.get("objects").is_none()
                && member["count"] == one
        }
        Some("sacrificePermanent" | "destroyPermanent") => member["permanent"].is_object(),
        Some("revealCards" | "exileTargetCards" | "moveCards") => {
            member["cards"]["kind"] == "chosenTarget"
        }
        _ => false,
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

/// `performAction{action|actions, onPerformed, onDeclined}`: the mandatory
/// instruction runs, and its measured performance selects the outcome.
/// `If you do` keeps the body's declarations on the enclosing ability (CR
/// 608.2c); a reflexive ability declares its own targets when it is put on
/// the stack (CR 603.12). The returned declarations are the outcomes' own.
fn lower_performed_action(
    cursor: &mut OracleTokenCursor<'_>,
    action: Vec<Value>,
    outcome: PerformanceOutcome,
    body: LoweredEffectSequence,
    otherwise: Option<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.reflexive-action.action");
    let instruction =
        measured_outcome_instruction(cursor, &action, &body, OutcomeInstructionRole::Mandatory)?;
    if let Some(otherwise) = otherwise.as_ref() {
        measured_outcome_instruction(
            cursor,
            &action,
            otherwise,
            OutcomeInstructionRole::Mandatory,
        )?;
    }
    let mut decisions = Vec::new();
    let (performed, declined) = match outcome {
        PerformanceOutcome::Performed | PerformanceOutcome::Declined => {
            decisions.extend(body.decisions);
            let otherwise = otherwise.map(|otherwise| {
                decisions.extend(otherwise.decisions);
                otherwise.effects
            });
            complementary_effects(outcome, body.effects, otherwise)
        }
        PerformanceOutcome::Reflexive => (vec![reflexive_trigger(&controller(), body)], None),
    };
    cursor.enter("lower.program.reflexive-action.declarations");
    require_distinct_declarations(
        cursor,
        &decisions,
        "distinct declaration ids across the instruction and its outcome",
    )?;
    let mut effect = json!({
        "kind": "performAction",
        "onPerformed": performed,
    });
    instruction.install(&mut effect);
    if let Some(declined) = declined {
        effect["onDeclined"] = Value::Array(declined);
    }
    Ok(LoweredEffectSequence {
        decisions,
        effects: vec![effect],
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
        // Some damage was dealt (CR 120.1); prevented damage is not.
        Some("dealDamage") => instruction["recipient"].is_object(),
        Some("loseLife") => single_player(&instruction["player"]),
        // The permanent is controlled by the new controller afterwards.
        Some("gainControlPermanent") => {
            instruction["permanent"].is_object() && instruction["controller"].is_object()
        }
        // The attachment became attached to the recipient (CR 701.3b).
        Some("attachPermanent") => {
            instruction["attachment"].is_object() && instruction["to"].is_object()
        }
        // The object was still the permanent the effect applied to.
        Some("modifyPowerToughness" | "grantKeyword") => instruction["object"].is_object(),
        _ => false,
    }
}

/// `optionalAction{player, action, onPerformed, onDeclined}`. The action's
/// targets are declared by the enclosing ability; a reflexive outcome
/// declares its own when it is put on the stack (CR 603.12).
fn lower_optional_action(
    cursor: &mut OracleTokenCursor<'_>,
    player: Value,
    action: Vec<Value>,
    outcome: PerformanceOutcome,
    body: LoweredEffectSequence,
    otherwise: Option<LoweredEffectSequence>,
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.optional-action.action");
    // Both sentences were lowered independently; a runtime binding or
    // decision produced by the instruction must not be shadowed by its outcome.
    let instruction =
        measured_outcome_instruction(cursor, &action, &body, OutcomeInstructionRole::Optional)?;
    if let Some(otherwise) = otherwise.as_ref() {
        measured_outcome_instruction(cursor, &action, otherwise, OutcomeInstructionRole::Optional)?;
    }
    let mut decisions = Vec::new();
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
        "onPerformed": performed,
    });
    instruction.install(&mut effect);
    if let Some(declined) = declined {
        effect["onDeclined"] = Value::Array(declined);
    }
    Ok(LoweredEffectSequence {
        decisions,
        effects: vec![effect],
    })
}

/// `offerAction{players, action, onPerformed, onDeclined}` (CR 608.2d,
/// 101.4): the runtime offers the one instruction to each player of the
/// population in turn order, running it as that player's own, until one
/// actually performs it (measured like `optionalAction`); `onPerformed`
/// then resolves, else `onDeclined`, both as the controller's instructions.
/// The instruction declares nothing: a target declared while the ability is
/// put on the stack would be the controller's, never the offered player's.
fn lower_offered_action(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
    action: LoweredEffectSequence,
    outcomes: [Option<LoweredEffectSequence>; 2],
) -> MachineResult<LoweredEffectSequence> {
    cursor.enter("lower.program.offer.action");
    let [instruction] = action.effects.as_slice() else {
        return Err(cursor.fail("exactly one offered instruction"));
    };
    cursor.require(
        action.decisions.is_empty() && instruction.get("performer").is_none(),
        "an offered instruction without declarations, performed by the offered player",
    )?;
    // An offer keeps the witnesses it was proven with; the witnesses added
    // for a performer's own instruction (damage, life loss, control, attach,
    // a modified object) are not offered to other players yet.
    if !optional_action_measured(instruction)
        || matches!(
            instruction["kind"].as_str(),
            Some(
                "dealDamage"
                    | "loseLife"
                    | "gainControlPermanent"
                    | "attachPermanent"
                    | "modifyPowerToughness"
                    | "grantKeyword"
            )
        )
    {
        return Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::MeasuredPerformance,
            "an offered instruction whose performance the runtime measures",
        ));
    }
    cursor.enter("lower.program.offer.outcomes");
    cursor.require(
        outcomes.iter().any(Option::is_some),
        "an outcome of the offer (`If a player does` or `If no one does`)",
    )?;
    let instruction_names = local_result_names(instruction);
    let mut decisions = Vec::new();
    let mut branches = Vec::with_capacity(2);
    for outcome in outcomes {
        let Some(body) = outcome else {
            branches.push(None);
            continue;
        };
        cursor.require(
            !body.effects.is_empty()
                && body
                    .effects
                    .iter()
                    .all(|effect| local_result_names(effect).is_disjoint(&instruction_names)),
            "a nonempty offer outcome with local results distinct from the instruction's",
        )?;
        decisions.extend(body.decisions);
        branches.push(Some(body.effects));
    }
    cursor.enter("lower.program.offer.declarations");
    require_distinct_declarations(
        cursor,
        &decisions,
        "distinct declaration ids across the offer's outcomes",
    )?;
    // CR 102.3: `any other player` is every player but the controller.
    let players = if excluding_controller {
        json!({"kind": "playersExcept", "player": controller()})
    } else {
        lower_player_set_reference(players)
    };
    let mut effect = json!({
        "kind": "offerAction",
        "players": players,
        "action": instruction,
    });
    for (key, branch) in ["onPerformed", "onDeclined"].into_iter().zip(branches) {
        if let Some(effects) = branch {
            effect[key] = Value::Array(effects);
        }
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
        EffectProgramNode::Offer { .. } => {
            cursor.enter("lower.program.offer");
            Err(cursor.fail("an offer lowered with its outcome sentences"))
        }
        EffectProgramNode::CreatedReplacement { .. } => {
            cursor.enter("lower.program.created-replacement");
            Err(cursor.fail("a created replacement lowered as its own program step"))
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
            ..
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
            // `you may ACTION if CONDITION`: the condition gates the
            // permission itself, so the performer is asked only when it holds.
            if let [conditional] = lowered.effects.as_slice()
                && conditional["kind"] == "conditional"
                && conditional["else"].as_array().is_some_and(Vec::is_empty)
                && conditional["then"]
                    .as_array()
                    .is_some_and(|then| !then.is_empty())
            {
                let mut conditional = conditional.clone();
                let then = conditional["then"].take();
                conditional["then"] = json!([{
                    "kind": "optionalEffects", "player": player,
                    "effects": then,
                }]);
                return Ok(LoweredEffectSequence {
                    decisions: lowered.decisions,
                    effects: vec![conditional],
                });
            }
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
