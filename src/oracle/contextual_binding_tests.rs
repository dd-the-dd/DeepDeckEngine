use serde_json::json;

use super::{
    contextual_binding::validate_native_contextual_bindings,
    model::{
        AbilitySource, OracleCardFace, OracleCardParseRequest, ParsedOracleAbility,
        RecognizerProvenance, SimplificationIteration,
    },
};

fn request(mana_cost: &str, faces: Vec<OracleCardFace>) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: "Binding Probe".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some("Destroy all creatures with mana value X or less.".to_string()),
        layout: Some(if faces.is_empty() { "normal" } else { "split" }.to_string()),
        faces,
    }
}

fn consumer(face_id: Option<&str>, explicit_declaration: bool) -> ParsedOracleAbility {
    let mut rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [{
            "kind": "destroyPermanent",
            "permanent": {
                "kind": "eachPermanent",
                "where": {
                    "kind": "compare",
                    "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                    "operator": "<=",
                    "right": { "kind": "decisionResult", "decisionId": "xValue" }
                }
            }
        }]
    });
    if explicit_declaration {
        rule["declaration"] = json!({
            "decisions": [{
                "kind": "chooseNumber",
                "id": "xValue",
                "minimum": { "kind": "integer", "value": 0 }
            }]
        });
    }
    ParsedOracleAbility {
        source: AbilitySource {
            face_id: face_id.map(ToOwned::to_owned),
            text: "Destroy all creatures with mana value X or less.".to_string(),
            line_start: 0,
            line_end: 0,
        },
        ability_type: "spellAbility".to_string(),
        status: "canonical".to_string(),
        recognizer: RecognizerProvenance::Native,
        rule: Some(rule),
        entities: Vec::new(),
        iterations: vec![SimplificationIteration {
            depth: 1,
            id: "pre-validation-rule".to_string(),
            operation: "Lower a provisional rule".to_string(),
            result: json!({ "kind": "spellAbility" }),
            result_text: "provisional".to_string(),
            status: "canonical".to_string(),
            title: "Provisional Rule IR".to_string(),
        }],
        diagnostics: Vec::new(),
    }
}

#[test]
fn same_face_mana_x_supplies_the_implicit_binding() {
    let mut abilities = vec![consumer(None, false)];
    validate_native_contextual_bindings(&request("{X}{B}", Vec::new()), &mut abilities);
    assert_eq!(abilities[0].status, "canonical");
    assert!(abilities[0].rule.is_some());
}

#[test]
fn explicit_declaration_supplies_x_without_mana_x() {
    let mut abilities = vec![consumer(None, true)];
    validate_native_contextual_bindings(&request("{2}{B}", Vec::new()), &mut abilities);
    assert_eq!(abilities[0].status, "canonical");
}

#[test]
fn x_on_another_face_does_not_supply_the_consumer() {
    let faces = vec![
        OracleCardFace {
            id: "front".to_string(),
            name: "Front".to_string(),
            type_line: "Sorcery".to_string(),
            mana_cost: Some("{2}{B}".to_string()),
            oracle_text: "Destroy all creatures with mana value X or less.".to_string(),
            power: None,
            toughness: None,
            loyalty: None,
        },
        OracleCardFace {
            id: "back".to_string(),
            name: "Back".to_string(),
            type_line: "Sorcery".to_string(),
            mana_cost: Some("{X}{G}".to_string()),
            oracle_text: "Destroy all lands.".to_string(),
            power: None,
            toughness: None,
            loyalty: None,
        },
    ];
    let mut abilities = vec![consumer(Some("front"), false)];
    validate_native_contextual_bindings(&request("{2}{B} // {X}{G}", faces), &mut abilities);
    assert_eq!(abilities[0].status, "unsupported");
    assert!(abilities[0].rule.is_none());
    assert!(abilities[0].iterations.is_empty());
    assert_eq!(
        abilities[0].diagnostics[0].state.as_deref(),
        Some("binding.x-value.source")
    );
}
