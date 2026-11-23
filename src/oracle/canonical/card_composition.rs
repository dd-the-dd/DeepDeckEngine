use serde_json::{Value, json};

use crate::oracle::model::{ParsedOracleAbility, ParserDiagnostic};

const OVERLOAD_MODE: &str = "overload";

/// The `link` value of `cards exiled with this NOUN`: every exile of the
/// object (CR 607.2a links each exiling ability with the reader).
const FACE_LINK: &str = "face";
/// The `link` value of `the exiled card(s)`: the one exile it is paired with.
const PAIR_LINK: &str = "pair";

/// Marks every exile of permanents that does not already return them when
/// the source leaves (that duration records the link itself).
fn link_permanent_exiles(value: &mut Value) -> usize {
    match value {
        Value::Object(map) => {
            let own = usize::from(
                map.get("kind").and_then(Value::as_str) == Some("exilePermanent")
                    && !map.contains_key("until"),
            );
            if own == 1 {
                map.insert("linkToSource".to_string(), Value::Bool(true));
            }
            own + map.values_mut().map(link_permanent_exiles).sum::<usize>()
        }
        Value::Array(items) => items.iter_mut().map(link_permanent_exiles).sum(),
        _ => 0,
    }
}

fn reject(ability: &mut ParsedOracleAbility, state: &str, message: &str, expected: &str) {
    ability.status = "unsupported".to_string();
    ability.rule = None;
    ability.diagnostics.push(ParserDiagnostic {
        code: "oracle_card_composition_failure".to_string(),
        message: message.to_string(),
        severity: "unsupported".to_string(),
        automaton: Some("linked-exile-composition".to_string()),
        state: Some(state.to_string()),
        token_index: None,
        span_start: Some(0),
        span_end: Some(ability.source.text.len()),
        expected: Some(expected.to_string()),
        found: None,
        trace: Vec::new(),
    });
}

/// Linked abilities (CR 607.2a): an ability that exiles and an ability of the
/// same object that refers to `the exiled card(s)` / cards `exiled with` it.
/// The reader is paired with the exile of its own ability, else with the one
/// other ability of its face that exiles permanents; only that exile records
/// the link (`linkToSource`). Two candidate abilities make the pairing
/// ambiguous and the reader fails explicitly; a reader lowered from `the
/// exiled card(s)` without any producer fails too. An exile of another kind
/// on a reader's face records no source relation and is rejected.
fn link_exiles_to_linked_readers(abilities: &mut [ParsedOracleAbility]) {
    let mentions = |rule: &Value, needle: &str| rule.to_string().contains(needle);
    let reads_linked = |rule: &Value| mentions(rule, "\"cardsExiledWithSource\"");
    let reads_face_link =
        |rule: &Value| reads_linked(rule) && mentions(rule, &format!("\"link\":\"{FACE_LINK}\""));
    let reads_pair_link =
        |rule: &Value| reads_linked(rule) && mentions(rule, &format!("\"link\":\"{PAIR_LINK}\""));
    let exiles_permanents = |rule: &Value| mentions(rule, "\"kind\":\"exilePermanent\"");
    let other_exiles = |rule: &Value| {
        mentions(rule, "\"exileTargetCards\"") || mentions(rule, "\"kind\":\"exile\"")
    };
    let faces_with_readers = abilities
        .iter()
        .filter(|ability| ability.rule.as_ref().is_some_and(reads_linked))
        .map(|ability| ability.source.face_id.clone())
        .collect::<Vec<_>>();
    for ability in abilities.iter_mut() {
        let Some(rule) = ability.rule.as_ref() else {
            continue;
        };
        if faces_with_readers.contains(&ability.source.face_id)
            && !reads_linked(rule)
            && other_exiles(rule)
        {
            reject(
                ability,
                "composition.linked-exile.relation",
                "A same-face ability reads cards exiled with this object (CR 607), but this exile records no source relation.",
                "an exile that records its source relation for the linked reader",
            );
        }
    }
    let readers = (0..abilities.len())
        .filter(|index| abilities[*index].rule.as_ref().is_some_and(reads_linked))
        .collect::<Vec<_>>();
    for reader in readers {
        // The reader's own exile is its pair.
        if abilities[reader]
            .rule
            .as_ref()
            .is_some_and(exiles_permanents)
        {
            if let Some(rule) = abilities[reader].rule.as_mut() {
                link_permanent_exiles(rule);
            }
            continue;
        }
        let face = abilities[reader].source.face_id.clone();
        let candidates = (0..abilities.len())
            .filter(|other| {
                *other != reader
                    && abilities[*other].source.face_id == face
                    && abilities[*other]
                        .rule
                        .as_ref()
                        .is_some_and(|rule| exiles_permanents(rule) && !reads_linked(rule))
            })
            .collect::<Vec<_>>();
        let pair = abilities[reader].rule.as_ref().is_some_and(reads_pair_link);
        match candidates.as_slice() {
            // `cards exiled with this` reads every exile of the object.
            producers if !producers.is_empty() && !pair => {
                for producer in producers {
                    if let Some(rule) = abilities[*producer].rule.as_mut() {
                        link_permanent_exiles(rule);
                    }
                }
            }
            [producer] => {
                if let Some(rule) = abilities[*producer].rule.as_mut() {
                    link_permanent_exiles(rule);
                }
            }
            [] if abilities[reader]
                .rule
                .as_ref()
                .is_some_and(|rule| reads_face_link(rule) || pair) =>
            {
                reject(
                    &mut abilities[reader],
                    "binding.linked-exile.producer",
                    "The ability refers to cards exiled with this object, but no ability of this face exiles permanents with it (CR 607.2a).",
                    "an ability of this face that exiles permanents with this object",
                )
            }
            [] => {}
            _ => reject(
                &mut abilities[reader],
                "binding.linked-exile.ambiguous",
                "The ability refers to cards exiled with this object, but several abilities of this face exile permanents; the linked pair is ambiguous (CR 607.2a).",
                "exactly one ability of this face that exiles permanents with this object",
            ),
        }
    }
}

/// `another target X` (or `other target X`, `each of two other target X`)
/// after an earlier target slot of the same ability names objects other
/// than that slot's (CR 115.3; Deadshot: `Tap target creature. It deals
/// damage … to another target creature.`). The slot already excludes its
/// source; each pair of target ids across the two slots is recorded as a
/// distinct target group the declaration must respect. A slot of several
/// objects keys each object `id:index`.
fn mark_distinct_other_targets(rule: &mut Value) {
    const MAX_DISTINCT_SLOT_SIZE: i64 = 10;
    let Some(decisions) = rule["declaration"]["decisions"].as_array() else {
        return;
    };
    let slot_ids = |decision: &Value| -> Option<Vec<String>> {
        let id = decision["id"].as_str()?;
        match decision["maximum"].as_i64()? {
            1 => Some(vec![id.to_string()]),
            maximum @ 2..=MAX_DISTINCT_SLOT_SIZE => {
                Some((0..maximum).map(|index| format!("{id}:{index}")).collect())
            }
            _ => None,
        }
    };
    let slots = decisions
        .iter()
        .filter(|decision| decision["kind"].as_str() == Some("chooseTargets"))
        .filter_map(|decision| {
            Some((
                slot_ids(decision)?,
                decision["candidates"]["excludeSource"].as_bool() == Some(true),
            ))
        })
        .collect::<Vec<_>>();
    let mut groups = Vec::new();
    for (index, (later_ids, other)) in slots.iter().enumerate() {
        if !other {
            continue;
        }
        for (earlier_ids, _) in &slots[..index] {
            for earlier in earlier_ids {
                for later in later_ids {
                    groups.push(json!([earlier, later]));
                }
            }
        }
    }
    if groups.is_empty() {
        return;
    }
    if let Some(existing) = rule["distinctTargets"].as_array() {
        for group in existing {
            if !groups.contains(group) {
                groups.push(group.clone());
            }
        }
    }
    rule["distinctTargets"] = Value::Array(groups);
}

pub(crate) fn compose_card_rules_native(abilities: &mut [ParsedOracleAbility]) {
    for ability in abilities.iter_mut() {
        if let Some(rule) = ability.rule.as_mut() {
            mark_distinct_other_targets(rule);
        }
    }
    link_exiles_to_linked_readers(abilities);
    let directives = abilities
        .iter()
        .enumerate()
        .filter_map(|(index, ability)| {
            let rule = ability.rule.as_ref()?;
            (rule["kind"].as_str() == Some("keywordAbility")
                && rule["ability"]["kind"].as_str() == Some("alternativeCost")
                && rule["ability"]["mode"].as_str() == Some(OVERLOAD_MODE)
                && rule["ability"]["compileDirective"]["kind"].as_str()
                    == Some("replaceTargetWithEach"))
            .then_some(index)
        })
        .collect::<Vec<_>>();

    for directive_index in directives {
        let mut composed = abilities.to_vec();
        match compile_target_to_each(&mut composed) {
            Ok(()) => {
                if let Some(rule) = composed[directive_index].rule.as_mut()
                    && let Some(ability) = rule["ability"].as_object_mut()
                {
                    ability.remove("compileDirective");
                }
                abilities.clone_from_slice(&composed);
            }
            Err(failure) => {
                // A diagnostic's recognizer is not proof that its spell body
                // was compiled. Preserve the independently recognized keyword
                // while a same-face sibling remains unsupported, whether no
                // child claimed it or a native child reports its exact limit.
                // The card itself remains unsupported in either case.
                let missing_unsupported_sibling =
                    abilities.iter().enumerate().any(|(index, ability)| {
                        index != directive_index
                            && ability.source.face_id == abilities[directive_index].source.face_id
                            && ability.status == "unsupported"
                            && ability.rule.is_none()
                    });
                if failure.state == "composition.target-rule" && missing_unsupported_sibling {
                    if let Some(rule) = abilities[directive_index].rule.as_mut()
                        && let Some(ability) = rule["ability"].as_object_mut()
                    {
                        ability.remove("compileDirective");
                    }
                    continue;
                }
                let source = &abilities[directive_index].source;
                abilities[directive_index].status = "unsupported".to_string();
                abilities[directive_index].rule = None;
                abilities[directive_index]
                    .diagnostics
                    .push(ParserDiagnostic {
                        code: "oracle_card_composition_failure".to_string(),
                        message: failure.message.clone(),
                        severity: "unsupported".to_string(),
                        automaton: Some("card-target-to-each-composition".to_string()),
                        state: Some(failure.state.to_string()),
                        token_index: None,
                        span_start: Some(0),
                        span_end: Some(source.text.len()),
                        expected: Some(failure.expected.to_string()),
                        found: failure.found,
                        trace: Vec::new(),
                    });
            }
        }
    }
}

struct CompositionFailure {
    state: &'static str,
    expected: &'static str,
    found: Option<String>,
    message: String,
}

fn compile_target_to_each(abilities: &mut [ParsedOracleAbility]) -> Result<(), CompositionFailure> {
    let overload_paid = json!({
        "kind": "wasCastWithAlternativeCost",
        "mode": OVERLOAD_MODE,
    });
    let normal_cast = json!({
        "kind": "not",
        "operand": overload_paid.clone(),
    });
    let mut transformed_rules = 0_usize;

    for ability in abilities {
        let Some(rule) = ability.rule.as_mut() else {
            continue;
        };
        if rule["kind"].as_str() != Some("spellAbility") {
            continue;
        }
        let Some(decisions) = rule["declaration"]["decisions"].as_array_mut() else {
            continue;
        };
        let mut substitutions = Vec::new();
        for decision in decisions
            .iter_mut()
            .filter(|decision| decision["kind"].as_str() == Some("chooseTargets"))
        {
            let decision_id = decision["id"].as_str().ok_or_else(|| CompositionFailure {
                state: "composition.target-decision.id",
                expected: "a stable target decision id",
                found: None,
                message: "Overload found a target decision without a stable id.".to_string(),
            })?;
            let collection = target_candidates_to_collection(&decision["candidates"])?;
            substitutions.push((decision_id.to_string(), collection));
            let previous = decision.get("condition").cloned();
            decision["condition"] = previous.map_or_else(
                || normal_cast.clone(),
                |condition| {
                    json!({
                        "kind": "and",
                        "operands": [condition, normal_cast.clone()],
                    })
                },
            );
        }
        if substitutions.is_empty() {
            continue;
        }

        let normal_effects = rule["effects"].clone();
        let mut overloaded_effects = normal_effects.clone();
        for (decision_id, collection) in &substitutions {
            if replace_target_references(&mut overloaded_effects, decision_id, collection) == 0 {
                return Err(CompositionFailure {
                    state: "composition.target-reference",
                    expected: "an effect reference to every transformed target decision",
                    found: Some(decision_id.clone()),
                    message: format!(
                        "Overload could not link target decision `{decision_id}` to a structured effect reference."
                    ),
                });
            }
        }
        rule["effects"] = json!([{
            "kind": "conditionalEffect",
            "condition": overload_paid.clone(),
            "then": overloaded_effects,
            "else": normal_effects,
        }]);
        transformed_rules += 1;
    }

    if transformed_rules == 0 {
        return Err(CompositionFailure {
            state: "composition.target-rule",
            expected: "at least one structured targeted spell rule",
            found: None,
            message: "Overload requires a structured targeted spell rule on the same card."
                .to_string(),
        });
    }
    Ok(())
}

fn target_candidates_to_collection(candidates: &Value) -> Result<Value, CompositionFailure> {
    if candidates["kind"].as_str() != Some("permanents") {
        return Err(CompositionFailure {
            state: "composition.selector-domain",
            expected: "a permanent target selector with a reusable set representation",
            found: candidates["kind"].as_str().map(ToOwned::to_owned),
            message: "This target domain does not yet have a target-to-each collection lowering."
                .to_string(),
        });
    }

    let mut collection = json!({ "kind": "eachPermanent" });
    if let Some(where_filter) = candidates.get("where").filter(|filter| !filter.is_null()) {
        collection["where"] = where_filter.clone();
    }
    if let Some(controller_scope) = candidates.get("controller") {
        match controller_scope["kind"].as_str() {
            Some("controllerOf") => collection["player"] = controller_scope.clone(),
            Some("playersExcept")
                if controller_scope["player"]["kind"].as_str() == Some("controllerOf") =>
            {
                collection["excludeSourceController"] = Value::Bool(true);
            }
            other => {
                return Err(CompositionFailure {
                    state: "composition.selector-controller",
                    expected: "controllerOf(self) or playersExcept(controllerOf(self))",
                    found: other.map(ToOwned::to_owned),
                    message:
                        "This controller relation does not yet have an exact target-to-each lowering."
                            .to_string(),
                });
            }
        }
    }
    Ok(collection)
}

fn replace_target_references(value: &mut Value, decision_id: &str, replacement: &Value) -> usize {
    if matches!(
        value["kind"].as_str(),
        Some("chosenTarget" | "chosenTargets")
    ) && value["id"].as_str() == Some(decision_id)
    {
        *value = replacement.clone();
        return 1;
    }

    match value {
        Value::Array(values) => values
            .iter_mut()
            .map(|value| replace_target_references(value, decision_id, replacement))
            .sum(),
        Value::Object(values) => values
            .values_mut()
            .map(|value| replace_target_references(value, decision_id, replacement))
            .sum(),
        _ => 0,
    }
}

pub(crate) fn compose_card_rules(abilities: &mut [ParsedOracleAbility]) {
    compose_card_rules_native(abilities);
}
