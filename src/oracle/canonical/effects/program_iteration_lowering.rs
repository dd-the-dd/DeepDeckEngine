//! Lowering of program-control frames over bound sets (CR 608.2c).
//!
//! A `For each PLAYER, BODY.` frame becomes the shared `forEachPlayer` loop:
//! every acting-player placeholder its body read (`that player`, `that
//! opponent`, `they`) names the bound member. A body target restricted to
//! permanents the member controls is declared once per member while the
//! ability is put on the stack (CR 601.2c): the slot admits one permanent of
//! each member, the members' permanents having distinct controllers, and the
//! body is performed for each declared permanent in turn.
//!
//! A `For each of SET, BODY.` frame becomes the shared `forEachObject` loop
//! over the objects one plural target slot declared; the body's singular
//! reference to that slot names the bound member.
//!
//! `repeat this process` lowers to a step that asks the runtime to perform
//! the process again; the rule drafter wraps the process, the program read
//! before that step, into one `repeatableProcess` (`wrap_repeated_processes`).
use std::cmp::Reverse;

use super::*;

fn limit(cursor: &OracleTokenCursor<'_>, expected: &str) -> EffectSequenceLoweringFailure {
    EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(expected))
}

fn is_pending_member(value: &Value) -> bool {
    value.get("kind").and_then(Value::as_str) == Some(ACTING_PLAYER)
}

fn contains_pending_member(value: &Value) -> bool {
    match value {
        Value::Object(map) => is_pending_member(value) || map.values().any(contains_pending_member),
        Value::Array(items) => items.iter().any(contains_pending_member),
        _ => false,
    }
}

fn contains_value(value: &Value, needle: &Value) -> bool {
    value == needle
        || match value {
            Value::Object(map) => map.values().any(|child| contains_value(child, needle)),
            Value::Array(items) => items.iter().any(|child| contains_value(child, needle)),
            _ => false,
        }
}

/// Replaces every occurrence of `from` under `value` by `to`.
fn replace_value(value: &mut Value, from: &Value, to: &Value) {
    if value == from {
        *value = to.clone();
        return;
    }
    match value {
        Value::Object(map) => map
            .values_mut()
            .for_each(|child| replace_value(child, from, to)),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| replace_value(child, from, to)),
        _ => {}
    }
}

/// The player slot a primitive's operands may name its own acting player
/// through (`each player loses 1 life for each creature they control`).
fn names_population(reference: &Value) -> bool {
    matches!(
        reference.get("kind").and_then(Value::as_str),
        Some("eachPlayer" | "opponentsOf" | "playersExcept" | "filterPlayers")
    )
}

/// A primitive of the body performed by a population of its own whose
/// operands read a distributive player: that `they` may be the nearer
/// population's member, not the iterated one; the reading is refused.
fn nests_another_population(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            let slot = match map.get("kind").and_then(Value::as_str) {
                Some(
                    "gainLife" | "loseLife" | "setLifeTotal" | "drawCards" | "mill"
                    | "discardCards" | "addPlayerCounters",
                ) => map.get("player"),
                Some("dealDamage") => map.get("recipient"),
                _ => None,
            };
            slot.is_some_and(names_population) && contains_pending_member(value)
                || map.values().any(nests_another_population)
        }
        Value::Array(items) => items.iter().any(nests_another_population),
        _ => false,
    }
}

fn population_reference(population: IterationPopulation) -> Value {
    match population {
        IterationPopulation::Players => json!({"kind": "eachPlayer"}),
        IterationPopulation::Opponents => json!({"kind": "opponentsOf", "player": controller()}),
        IterationPopulation::OtherPlayers => {
            json!({"kind": "playersExcept", "player": controller()})
        }
    }
}

/// How many members a population has as the ability is put on the stack.
fn population_count(population: IterationPopulation) -> Value {
    match population {
        IterationPopulation::Players => json!({"kind": "countPlayers"}),
        IterationPopulation::Opponents => json!({"kind": "countOpponents", "player": controller()}),
        IterationPopulation::OtherPlayers => json!({
            "kind": "subtract",
            "left": {"kind": "countPlayers"},
            "right": integer(1),
        }),
    }
}

/// The ids every top-level step of `body` binds that a later step of the
/// program reads: each member's binding is local to its pass, so the loop
/// republishes the union of all passes under the same id.
fn read_after_loop(body: &[Value], after: &[Value]) -> Vec<String> {
    // A step names its own result (`createTokens` with `bind`), or is the
    // binding step of a choice made on resolution (`bind` with `id`).
    let produced = |effect: &Value| match effect["kind"].as_str() {
        Some("bind") => effect["id"].as_str(),
        _ => effect["bind"].as_str(),
    };
    let mut ids = body
        .iter()
        .filter_map(produced)
        .filter(|id| mentions_identifier(after, id))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.dedup();
    ids
}

fn loop_collection(
    cursor: &OracleTokenCursor<'_>,
    body: &[Value],
    after: &[Value],
) -> Result<Option<Value>, EffectSequenceLoweringFailure> {
    match read_after_loop(body, after).as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(json!({
            "bind": id,
            "objects": {"kind": "boundObjects", "binding": id},
        }))),
        _ => Err(limit(
            cursor,
            "one result of the iterated body that a later instruction reads",
        )),
    }
}

/// While the body of a `For each of SET` frame is lowered, its singular
/// reference to the iterated slot (`that creature`, `it`) names the slot's
/// declaration, which the frame then rebinds to each member in turn. The
/// mapping exists only inside the frame's range.
pub(super) fn bind_iteration_members(
    frames: &[IterationFrameNode],
    effect_index: usize,
    target_bindings: &mut Vec<(TargetBinding, String)>,
) {
    for frame in frames {
        let IterationDomain::DeclaredObjects(slot) = frame.domain else {
            continue;
        };
        if effect_index == frame.end_effect {
            target_bindings.retain(|(binding, _)| *binding != slot);
        }
        if effect_index == frame.first_effect {
            let plural = super::super::permanent_subject::target_set_binding_key(slot);
            if let Some(id) = target_bindings
                .iter()
                .find(|(binding, _)| *binding == plural)
                .map(|(_, id)| id.clone())
            {
                target_bindings.push((slot, id));
            }
        }
    }
}

/// Wraps every iteration frame's lowered range into its loop.
pub(super) fn apply_iteration_frames(
    cursor: &mut OracleTokenCursor<'_>,
    frames: &[IterationFrameNode],
    target_bindings: &mut Vec<(TargetBinding, String)>,
    action_starts: &mut [(usize, usize)],
    decisions: &mut [Value],
    effects: &mut Vec<Value>,
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut frames = frames.to_vec();
    frames.sort_by_key(|frame| Reverse(frame.first_effect));
    for frame in frames {
        let (from, decisions_from) = action_starts[frame.first_effect];
        let (to, decisions_to) = action_starts[frame.end_effect];
        cursor.enter("lower.for-each.body");
        if from >= to {
            return Err(limit(
                cursor,
                "an iterated body that lowers to executable steps",
            ));
        }
        let body = effects[from..to].to_vec();
        let lowered = match frame.domain {
            IterationDomain::Players(population) => lower_player_iteration(
                cursor,
                frame.first_effect,
                population,
                body,
                &mut decisions[decisions_from..decisions_to],
                &effects[to..],
            )?,
            IterationDomain::DeclaredObjects(slot) => {
                let lowered = lower_member_iteration(
                    cursor,
                    frame.first_effect,
                    slot,
                    body,
                    target_bindings,
                    &effects[to..],
                )?;
                target_bindings.retain(|(binding, _)| *binding != slot);
                lowered
            }
        };
        effects.splice(from..to, [lowered]);
        for start in action_starts.iter_mut() {
            if start.0 >= to {
                start.0 = start.0 - (to - from) + 1;
            } else if start.0 > from {
                start.0 = from;
            }
        }
    }
    Ok(())
}

/// `For each PLAYER, BODY` (CR 608.2c, 101.4).
fn lower_player_iteration(
    cursor: &mut OracleTokenCursor<'_>,
    first_effect: usize,
    population: IterationPopulation,
    body: Vec<Value>,
    decisions: &mut [Value],
    after: &[Value],
) -> Result<Value, EffectSequenceLoweringFailure> {
    let per_player = decisions
        .iter()
        .enumerate()
        .filter(|(_, decision)| contains_pending_member(decision))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if !per_player.is_empty() {
        cursor.enter("lower.for-each-player.per-player-targets");
        let [index] = per_player.as_slice() else {
            return Err(limit(
                cursor,
                "one target slot declared for each player of the population",
            ));
        };
        let decision = &mut decisions[*index];
        distribute_target_decision(decision, population)
            .map_err(|expected| limit(cursor, expected))?;
        let id = decision["id"]
            .as_str()
            .ok_or_else(|| limit(cursor, "an identified target declaration"))?
            .to_owned();
        // The slot now holds a permanent of each member: a later singular
        // reference to it (`That creature's owner ...`) names no one object.
        cursor.enter("lower.for-each-player.per-player-targets.later-reference");
        if after
            .iter()
            .any(|effect| contains_value(effect, &chosen_target(&id)))
        {
            return Err(limit(
                cursor,
                "no singular reference after the frame to a target declared for each player",
            ));
        }
        return lower_per_player_target_body(cursor, first_effect, &id, body);
    }
    cursor.enter("lower.for-each-player.member");
    if body.iter().any(nests_another_population) {
        return Err(limit(
            cursor,
            "a body whose distributive player is not shadowed by a population of its own",
        ));
    }
    let bind = format!("iteratedPlayer{first_effect}");
    let member = json!({"kind": "boundValue", "id": bind});
    let mut body = body;
    for effect in &mut body {
        bind_acting_player(effect, &member);
    }
    let collect = loop_collection(cursor, &body, after)?;
    let mut lowered = json!({
        "kind": "forEachPlayer",
        "players": population_reference(population),
        "bind": bind,
        "effects": body,
    });
    if let Some(collect) = collect {
        lowered["collect"] = collect;
    }
    Ok(lowered)
}

/// The declaration of `up to one target PERMANENT that player controls`
/// inside `For each PLAYER`: one slot that admits at most one permanent of
/// each member (CR 601.2c, 115.1), as a set of permanents whose controllers
/// are distinct members of the population.
fn distribute_target_decision(
    decision: &mut Value,
    population: IterationPopulation,
) -> Result<(), &'static str> {
    if decision["kind"] != "chooseTargets" {
        return Err("a target declaration restricted to the iterated player");
    }
    if decision["candidates"]["kind"] != "permanents"
        || !is_pending_member(&decision["candidates"]["controller"])
        || decision["candidates"].as_object().is_some_and(|map| {
            map.iter()
                .any(|(key, value)| key != "controller" && contains_pending_member(value))
        })
    {
        return Err(
            "a target permanent the iterated player controls; a card in that player's zone has no per-player declaration",
        );
    }
    if decision["minimum"].as_i64() != Some(0) {
        return Err(
            "`up to one` target per player; an exact target per player (CR 601.2c) has no bounded declaration",
        );
    }
    if decision["maximum"].as_i64() != Some(1) {
        return Err("at most one target per player");
    }
    match population {
        IterationPopulation::Players => {
            if let Some(candidates) = decision["candidates"].as_object_mut() {
                candidates.remove("controller");
            }
        }
        population => {
            decision["candidates"]["controller"] = population_reference(population);
        }
    }
    decision["maximum"] = population_count(population);
    let distinct = json!({"kind": "distinctPermanentControllers"});
    decision["selectionConstraint"] = match decision.get("selectionConstraint").cloned() {
        None => distinct,
        Some(existing) => json!({"kind": "allOf", "constraints": [existing, distinct]}),
    };
    Ok(())
}

/// The body over the permanents a per-player slot declared: performed for
/// each declared permanent in turn, each pass reading its own permanent. A
/// body that names the player apart from that target has no member to read.
fn lower_per_player_target_body(
    cursor: &mut OracleTokenCursor<'_>,
    first_effect: usize,
    id: &str,
    body: Vec<Value>,
) -> Result<Value, EffectSequenceLoweringFailure> {
    cursor.enter("lower.for-each-player.per-player-targets.body");
    if body.iter().any(contains_pending_member) {
        return Err(limit(
            cursor,
            "a body that names the iterated player only through its per-player target",
        ));
    }
    let bind = format!("iteratedTarget{first_effect}");
    let member = json!({"kind": "boundValue", "id": bind});
    let singular = chosen_target(id);
    let mut body = body;
    for effect in &mut body {
        replace_value(effect, &singular, &member);
    }
    Ok(json!({
        "kind": "forEachObject",
        "objects": {"kind": "chosenTargets", "id": id},
        "bind": bind,
        "effects": body,
    }))
}

/// `For each of them|those NOUNS, BODY` over one plural target slot.
fn lower_member_iteration(
    cursor: &mut OracleTokenCursor<'_>,
    first_effect: usize,
    slot: TargetBinding,
    body: Vec<Value>,
    target_bindings: &[(TargetBinding, String)],
    after: &[Value],
) -> Result<Value, EffectSequenceLoweringFailure> {
    cursor.enter("lower.for-each-member.set");
    let plural = super::super::permanent_subject::target_set_binding_key(slot);
    let id = target_bindings
        .iter()
        .find(|(binding, _)| *binding == plural)
        .map(|(_, id)| id.clone())
        .ok_or_else(|| limit(cursor, "the declared plural target slot the frame iterates"))?;
    let bind = format!("iteratedObject{first_effect}");
    let member = json!({"kind": "boundValue", "id": bind});
    let singular = chosen_target(&id);
    let mut body = body;
    for effect in &mut body {
        replace_value(effect, &singular, &member);
    }
    cursor.enter("lower.for-each-member.reference");
    if !mentions_identifier(&body, &bind) {
        return Err(limit(cursor, "a body that names the iterated member"));
    }
    let collect = loop_collection(cursor, &body, after)?;
    let mut lowered = json!({
        "kind": "forEachObject",
        "objects": {"kind": "chosenTargets", "id": id},
        "bind": bind,
        "effects": body,
    });
    if let Some(collect) = collect {
        lowered["collect"] = collect;
    }
    Ok(lowered)
}

/// The id the runtime signals a repeated process under. One process per
/// rule: a second repetition fails in `wrap_repeated_processes`.
const REPEATED_PROCESS: &str = "repeatedProcess";

/// `repeat this process [once | N more times]` / `repeat the following
/// process N times`: the step the rule drafter resolves.
pub(super) fn lower_repeat_process(
    cursor: &mut OracleTokenCursor<'_>,
    node: &RepeatProcessNode,
) -> Result<Value, EffectSequenceLoweringFailure> {
    cursor.enter("lower.repeat-process");
    let (count, key) = match &node.process {
        RepeatedProcess::Preceding { additional } => (additional.as_ref(), "additional"),
        RepeatedProcess::Following { times } => (Some(times), "times"),
    };
    let mut marker = json!({"kind": "repeatProcess", "process": REPEATED_PROCESS});
    if let RepeatedProcess::Following { .. } = &node.process {
        marker["following"] = json!(true);
    }
    if let Some(count) = count {
        marker[key] = lower_numeric_expression(count).ok_or_else(|| {
            limit(
                cursor,
                "a repetition count the shared numeric lowering represents",
            )
        })?;
    }
    Ok(marker)
}

/// Keys whose values are other rules or rules performed later: a
/// repetition inside them never names the enclosing program.
const NESTED_RULE_KEYS: &[&str] = &["abilities", "ability", "rule", "rules", "trigger"];

fn is_delayed_body(value: &Value) -> bool {
    matches!(
        value.get("kind").and_then(Value::as_str),
        Some(
            "installDelayedStepTrigger"
                | "installDelayedTrigger"
                | "createReflexiveTrigger"
                | "createDelayedTrigger"
        )
    )
}

fn is_marker(value: &Value) -> bool {
    value.get("kind").and_then(Value::as_str) == Some("repeatProcess")
}

/// Counts the repetition steps under `value` that name its own program;
/// steps inside nested or later rules are marked unresolved instead.
fn count_markers(value: &mut Value, nested: bool) -> usize {
    if is_marker(value) {
        if nested {
            value["kind"] = json!(UNRESOLVED_REPETITION);
            return 0;
        }
        return 1;
    }
    let nested = nested || is_delayed_body(value);
    match value {
        Value::Object(map) => map
            .iter_mut()
            .map(|(key, child)| {
                count_markers(child, nested || NESTED_RULE_KEYS.contains(&key.as_str()))
            })
            .sum(),
        Value::Array(items) => items
            .iter_mut()
            .map(|child| count_markers(child, nested))
            .sum(),
        _ => 0,
    }
}

/// A repetition whose process could not be resolved: the runtime refuses
/// it, so the rule is never executable with a silent single performance.
const UNRESOLVED_REPETITION: &str = "unresolvedRepeatedProcess";

fn mark_unresolved(value: &mut Value) {
    if is_marker(value) {
        value["kind"] = json!(UNRESOLVED_REPETITION);
        return;
    }
    match value {
        Value::Object(map) => map.values_mut().for_each(mark_unresolved),
        Value::Array(items) => items.iter_mut().for_each(mark_unresolved),
        _ => {}
    }
}

/// Resolves `this process` against the rule's program (CR 608.2c): the
/// steps from the program's start through the top-level step that contains
/// the repetition are one `repeatableProcess`, performed again each time the
/// repetition is reached, at most `additional` more times when bounded. `the
/// following process` is the rest of the program after the step. A rule
/// with several repetitions, or a repetition this cannot resolve, keeps an
/// unresolved step the runtime refuses.
pub(in crate::oracle::canonical) fn wrap_repeated_processes(rule: &mut Value) {
    let Some(effects) = rule.get_mut("effects").and_then(Value::as_array_mut) else {
        mark_unresolved(rule);
        return;
    };
    let counts = effects
        .iter_mut()
        .map(|effect| count_markers(effect, false))
        .collect::<Vec<_>>();
    let total = counts.iter().sum::<usize>();
    if total == 0 {
        return;
    }
    let Some(index) = counts.iter().position(|count| *count > 0) else {
        return;
    };
    if total != 1 {
        effects.iter_mut().for_each(mark_unresolved);
        return;
    }
    if effects[index]["following"] == json!(true) {
        // `Repeat the following process N times.`: the step itself is the
        // whole sentence; the process is every later step.
        if index + 1 >= effects.len() {
            mark_unresolved(&mut effects[index]);
            return;
        }
        let times = effects[index]["times"].clone();
        let following = effects.split_off(index + 1);
        effects.truncate(index);
        effects.push(json!({
            "kind": "repeatEffects",
            "count": times,
            "effects": following,
        }));
        return;
    }
    let marker = find_marker(&effects[index]).cloned();
    // The process is the program read before the repetition: without an
    // earlier step (a quoted ability's only instruction) there is none.
    let Some(marker) = marker
        .filter(|marker| marker.get("following").is_none())
        .filter(|_| index > 0 || !is_marker(&effects[index]))
    else {
        effects.iter_mut().for_each(mark_unresolved);
        return;
    };
    let rest = effects.split_off(index + 1);
    let process = std::mem::take(effects);
    let mut repeatable = json!({
        "kind": "repeatableProcess",
        "process": marker["process"].clone(),
        "effects": process,
    });
    if let Some(additional) = marker.get("additional") {
        repeatable["maxRepeats"] = additional.clone();
    }
    effects.push(repeatable);
    effects.extend(rest);
}

fn find_marker(value: &Value) -> Option<&Value> {
    if is_marker(value) {
        return Some(value);
    }
    match value {
        Value::Object(map) => map.values().find_map(find_marker),
        Value::Array(items) => items.iter().find_map(find_marker),
        _ => None,
    }
}
