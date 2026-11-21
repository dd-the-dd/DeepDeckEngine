use serde_json::json;

use super::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};

#[test]
fn audit_sections_project_entities_from_the_original_token_spans() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Audit Grove".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some("{T}: Add {G}.".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    let first = &parsed.abilities[0].iterations[0].result;

    assert_eq!(first["source"]["entities"], json!([]));
    assert_eq!(first["costs"][0]["entities"], json!(["{T}"]));
    assert_eq!(first["effects"][0]["entities"], json!(["Add", "{G}"]));

    let original_entities = parsed.abilities[0]
        .entities
        .iter()
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    for section in ["costs", "effects"] {
        for entity in first[section][0]["entities"]
            .as_array()
            .expect("audit entity list")
        {
            assert!(original_entities.contains(&entity.as_str().expect("entity string")));
        }
    }
}

#[test]
fn native_audit_splits_parenthesized_activation_on_the_outer_colon() {
    let parsed = parse_oracle_card_native(OracleCardParseRequest {
        card_name: "Audit Grotto".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some("({T}: Add {R}.)".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    let first = &parsed.abilities[0].iterations[0].result;

    assert_eq!(first["costs"][0]["text"], "{T}");
    assert_eq!(first["effects"][0]["text"], "Add {R}");
}

#[test]
fn native_audit_projects_as_enters_event_from_existing_token_spans() {
    for oracle_text in [
        "As this artifact enters, choose a creature type.",
        "AS this artifact ENTERS, choose a creature type.",
    ] {
        let parsed = parse_oracle_card_native(OracleCardParseRequest {
            card_name: "Audit Artifact".to_string(),
            type_line: "Artifact".to_string(),
            mana_cost: None,
            oracle_text: Some(oracle_text.to_string()),
            layout: None,
            faces: Vec::new(),
        });
        let event = &parsed.abilities[0].iterations[0].result["event"];
        assert_eq!(event["text"], oracle_text.split_once(',').unwrap().0);
    }
}

#[test]
fn as_enters_projection_requires_the_comma_boundary() {
    let parsed = parse_oracle_card_native(OracleCardParseRequest {
        card_name: "Audit Artifact".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: None,
        oracle_text: Some("As this artifact enters choose a creature type.".to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "unsupported");
    assert_ne!(
        parsed.abilities[0].iterations[0].result["event"]["text"],
        "As this artifact enters"
    );
}
