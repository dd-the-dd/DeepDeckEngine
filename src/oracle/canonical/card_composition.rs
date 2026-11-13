use serde_json::{Value, json};

use crate::oracle::model::{ParsedOracleAbility, ParserDiagnostic};

const OVERLOAD_MODE: &str = "overload";

pub(crate) fn compose_card_rules(abilities: &mut [ParsedOracleAbility]) {
    compose_card_rules_with_policy(abilities, CardCompositionPolicy::Full);
}

pub(crate) fn compose_card_rules_native(abilities: &mut [ParsedOracleAbility]) {
    compose_card_rules_with_policy(abilities, CardCompositionPolicy::NativeOnly);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardCompositionPolicy {
    Full,
    NativeOnly,
}

fn compose_card_rules_with_policy(
    abilities: &mut [ParsedOracleAbility],
    policy: CardCompositionPolicy,
) {
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
                if policy == CardCompositionPolicy::NativeOnly
                    && failure.state == "composition.target-rule"
                    && missing_unsupported_sibling
                {
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
