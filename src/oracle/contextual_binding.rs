use serde_json::Value;

use crate::oracle::model::{
    OracleCardParseRequest, ParsedOracleAbility, ParserDiagnostic, RecognizerProvenance,
};

const X_VALUE: &str = "xValue";

/// Prefix of the `bind` ids that carry a postposed `where X is EXPR` value
/// (CR 107.3). The sentence that defines X is lowered without knowing its
/// face; this validator rejects a face that also supplies X by another
/// producer, so the two definitions are never merged or borrowed.
pub(crate) const WHERE_X_BINDING_ID: &str = "whereX";

/// Decision id of a creature type chosen while an effect program resolves
/// (CR 608.2d). Distinct from the `As ... enters, choose` replacement choice
/// stored on a permanent: only a `chooseCreatureType` action of the same rule
/// produces it, and the validator below rejects a rule without that producer.
pub(crate) const RESOLUTION_CREATURE_TYPE_DECISION: &str = "resolutionCreatureType";

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
        if binds_where_x(rule) {
            let face_mana_cost = mana_cost_for_source(request, ability.source.face_id.as_deref());
            let mana_x = face_mana_cost.is_some_and(|mana_cost| mana_cost.contains("{X}"));
            if mana_x || declares_number(rule, X_VALUE) {
                let source_span_end = ability.source.text.len();
                ability.status = "unsupported".to_string();
                ability.rule = None;
                ability.iterations.clear();
                ability.diagnostics.push(ParserDiagnostic {
                    code: "oracle_context_binding_failure".to_string(),
                    message: "A `where X is` definition binds X, but this face also supplies X through {X} in its mana cost or an explicit `xValue` declaration; the two producers are not merged.".to_string(),
                    severity: "unsupported".to_string(),
                    automaton: Some("card-context-binding".to_string()),
                    state: Some("binding.x-value.definition-conflict".to_string()),
                    token_index: None,
                    span_start: Some(0),
                    span_end: Some(source_span_end),
                    expected: Some(
                        "a face whose only producer of X is the `where X is` definition"
                            .to_string(),
                    ),
                    found: Some(if mana_x {
                        face_mana_cost.unwrap_or_default().to_string()
                    } else {
                        "an explicit xValue declaration".to_string()
                    }),
                    trace: Vec::new(),
                });
                continue;
            }
        }
        if let Some((state, expected)) = resolution_creature_type_binding_failure(rule) {
            let source_span_end = ability.source.text.len();
            ability.status = "unsupported".to_string();
            ability.rule = None;
            ability.iterations.clear();
            ability.diagnostics.push(ParserDiagnostic {
                code: "oracle_context_binding_failure".to_string(),
                message: "The canonical rule reads a creature type chosen on resolution without a proven producer or executable consumer.".to_string(),
                severity: "unsupported".to_string(),
                automaton: Some("card-context-binding".to_string()),
                state: Some(state.to_string()),
                token_index: None,
                span_start: Some(0),
                span_end: Some(source_span_end),
                expected: Some(expected.to_string()),
                found: Some("a chosen creature type reference".to_string()),
                trace: Vec::new(),
            });
            continue;
        }
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

fn binds_where_x(value: &Value) -> bool {
    if value["kind"].as_str() == Some("bind")
        && value["id"]
            .as_str()
            .is_some_and(|id| id.starts_with(WHERE_X_BINDING_ID))
    {
        return true;
    }
    match value {
        Value::Array(values) => values.iter().any(binds_where_x),
        Value::Object(values) => values.values().any(binds_where_x),
        _ => false,
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

fn is_resolution_creature_type(value: &Value) -> bool {
    value["kind"].as_str() == Some("chosenCreatureType")
        && value["decisionId"].as_str() == Some(RESOLUTION_CREATURE_TYPE_DECISION)
}

/// Counts references to the resolution creature-type choice, and those read
/// by a battlefield count (the engine consumer that reads resolution choices).
fn resolution_creature_type_references(value: &Value, inside_count: bool) -> (usize, usize) {
    if is_resolution_creature_type(value) {
        return (1, usize::from(inside_count));
    }
    let counted = value["kind"].as_str() == Some("countPermanents");
    let children: Box<dyn Iterator<Item = (&String, &Value)>> = match value {
        Value::Object(values) => Box::new(values.iter()),
        Value::Array(values) => {
            return values.iter().fold((0, 0), |(total, read), value| {
                let (more, more_read) = resolution_creature_type_references(value, inside_count);
                (total + more, read + more_read)
            });
        }
        _ => return (0, 0),
    };
    children.fold((0, 0), |(total, read), (key, value)| {
        let (more, more_read) =
            resolution_creature_type_references(value, inside_count || (counted && key == "where"));
        (total + more, read + more_read)
    })
}

pub(crate) fn references_resolution_creature_type(value: &Value) -> bool {
    resolution_creature_type_references(value, false).0 > 0
}

fn produces_resolution_creature_type(value: &Value) -> usize {
    let own = usize::from(
        value["kind"].as_str() == Some("chooseCreatureType")
            && value["id"].as_str() == Some(RESOLUTION_CREATURE_TYPE_DECISION),
    );
    own + match value {
        Value::Object(values) => values.values().map(produces_resolution_creature_type).sum(),
        Value::Array(values) => values.iter().map(produces_resolution_creature_type).sum(),
        _ => 0,
    }
}

/// CR 608.2d: a creature type chosen while the rule resolves is read only by
/// the same rule, after its one producing choice, through a consumer that the
/// engine evaluates against resolution choices.
fn resolution_creature_type_binding_failure(rule: &Value) -> Option<(&'static str, &'static str)> {
    let (references, counted) = resolution_creature_type_references(rule, false);
    if references == 0 {
        return None;
    }
    if produces_resolution_creature_type(rule) != 1 {
        return Some((
            "binding.chosen-creature-type.source",
            "exactly one `choose a creature type` action of the same rule",
        ));
    }
    (counted != references).then_some((
        "binding.chosen-creature-type.consumer",
        "a battlefield count reading the chosen creature type",
    ))
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
