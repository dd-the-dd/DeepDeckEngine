use serde_json::{Value, json};

use crate::oracle::model::{ParsedOracleAbility, ParserDiagnostic};

const OVERLOAD_MODE: &str = "overload";

/// The `link` value of a linked-set reader lowered from `the exiled card(s)`
/// or `cards exiled with this NOUN`.
const FACE_LINK: &str = "face";

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

/// Linked abilities (CR 607.2a): when one ability of a face reads the cards
/// "exiled with" this object, the face's other exiles must record that
/// relation. An exile of permanents records it (`linkToSource`); any other
/// exile has no such primitive and is rejected explicitly. A reader lowered
/// from `the exiled card(s)` whose face records no link is rejected too, so
/// it never reads a set that can only be empty.
fn link_exiles_to_linked_readers(abilities: &mut [ParsedOracleAbility]) {
    let mentions = |rule: &Value, needle: &str| rule.to_string().contains(needle);
    let reads_linked = |rule: &Value| mentions(rule, "\"cardsExiledWithSource\"");
    let reads_face_link =
        |rule: &Value| reads_linked(rule) && mentions(rule, &format!("\"link\":\"{FACE_LINK}\""));
    let other_exiles = |rule: &Value| {
        mentions(rule, "\"exileTargetCards\"") || mentions(rule, "\"kind\":\"exile\"")
    };
    let faces_with_readers = abilities
        .iter()
        .filter(|ability| ability.rule.as_ref().is_some_and(reads_linked))
        .map(|ability| ability.source.face_id.clone())
        .collect::<Vec<_>>();
    let mut linked_faces = Vec::new();
    for ability in abilities.iter_mut() {
        let Some(rule) = ability.rule.as_mut() else {
            continue;
        };
        if !faces_with_readers.contains(&ability.source.face_id) || reads_linked(rule) {
            continue;
        }
        if other_exiles(rule) {
            reject(
                ability,
                "composition.linked-exile.relation",
                "A same-face ability reads cards exiled with this object (CR 607), but this exile records no source relation.",
                "an exile that records its source relation for the linked reader",
            );
            continue;
        }
        if link_permanent_exiles(rule) > 0 {
            linked_faces.push(ability.source.face_id.clone());
        }
    }
    for ability in abilities.iter_mut() {
        if ability.rule.as_ref().is_some_and(reads_face_link)
            && !linked_faces.contains(&ability.source.face_id)
        {
            reject(
                ability,
                "binding.linked-exile.producer",
                "The ability refers to cards exiled with this object, but no other ability of this face exiles permanents with it (CR 607.2a).",
                "another ability of this face that exiles permanents with this object",
            );
        }
    }
}

pub(crate) fn compose_card_rules_native(abilities: &mut [ParsedOracleAbility]) {
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
