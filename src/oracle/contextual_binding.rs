use serde_json::Value;

use crate::oracle::model::{
    OracleCardParseRequest, ParsedOracleAbility, ParserDiagnostic, RecognizerProvenance,
};

const X_VALUE: &str = "xValue";

pub(crate) fn validate_native_contextual_bindings(
    request: &OracleCardParseRequest,
    abilities: &mut [ParsedOracleAbility],
) {
    for ability in abilities {
        if ability.status != "canonical" || ability.recognizer != RecognizerProvenance::Native {
            continue;
        }
        let Some(rule) = ability.rule.as_ref() else {
            continue;
        };
        if !references_decision(rule, X_VALUE) || declares_number(rule, X_VALUE) {
            continue;
        }

        let face_mana_cost = mana_cost_for_source(request, ability.source.face_id.as_deref());
        if rule["kind"].as_str() == Some("spellAbility")
            && face_mana_cost.is_some_and(|mana_cost| mana_cost.contains("{X}"))
        {
            continue;
        }

        let found = face_mana_cost
            .map(ToOwned::to_owned)
            .or_else(|| {
                ability
                    .source
                    .face_id
                    .as_ref()
                    .map(|face_id| format!("missing face `{face_id}`"))
            })
            .unwrap_or_else(|| "no mana cost".to_string());
        let source_span_end = ability.source.text.len();
        ability.status = "unsupported".to_string();
        ability.rule = None;
        ability.iterations.clear();
        ability.diagnostics.push(ParserDiagnostic {
            code: "oracle_context_binding_failure".to_string(),
            message: "The canonical rule consumes `xValue`, but this spell face has neither an explicit `xValue` declaration nor {X} in its mana cost.".to_string(),
            severity: "unsupported".to_string(),
            automaton: Some("card-context-binding".to_string()),
            state: Some("binding.x-value.source".to_string()),
            token_index: None,
            span_start: Some(0),
            span_end: Some(source_span_end),
            expected: Some(
                "an explicit xValue declaration or {X} in this spell face mana cost"
                    .to_string(),
            ),
            found: Some(found),
            trace: Vec::new(),
        });
    }
}

fn mana_cost_for_source<'a>(
    request: &'a OracleCardParseRequest,
    face_id: Option<&str>,
) -> Option<&'a str> {
    match face_id {
        Some(face_id) => request
            .faces
            .iter()
            .find(|face| face.id == face_id)
            .and_then(|face| face.mana_cost.as_deref()),
        None => request.mana_cost.as_deref(),
    }
}

fn references_decision(value: &Value, decision_id: &str) -> bool {
    if value["kind"].as_str() == Some("decisionResult")
        && value["decisionId"].as_str() == Some(decision_id)
    {
        return true;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| references_decision(value, decision_id)),
        Value::Object(values) => values
            .values()
            .any(|value| references_decision(value, decision_id)),
        _ => false,
    }
}

fn declares_number(rule: &Value, decision_id: &str) -> bool {
    rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|decision| {
            decision["kind"].as_str() == Some("chooseNumber")
                && decision["id"].as_str() == Some(decision_id)
        })
}
