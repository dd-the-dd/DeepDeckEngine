use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const HYDROBLAST_ORACLE: &str =
    "Choose one —\n• Counter target spell if it's red.\n• Destroy target permanent if it's red.";

#[test]
fn hydroblast_exact_oracle_is_native_color_guarded_modal_ir() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Hydroblast".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(HYDROBLAST_ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );

    let rule = ability.rule.as_ref().expect("primitive modal rule");
    assert_eq!(
        rule["declaration"]["decisions"][0],
        json!({
            "id": "chosenModes",
            "kind": "chooseModes",
            "minimum": 1,
            "maximum": { "kind": "integer", "value": 1 },
            "options": ["mode1", "mode2"],
            "allowRepeated": false,
        })
    );
    assert_eq!(
        rule["declaration"]["decisions"][1]["candidates"],
        json!({ "kind": "spells" })
    );
    assert_eq!(
        rule["declaration"]["decisions"][2]["candidates"],
        json!({ "kind": "permanents", "where": null })
    );
    assert_eq!(
        rule["effects"][0]["then"][0]["condition"]["where"],
        json!({ "kind": "colorContains", "value": "Red" })
    );
    assert_eq!(
        rule["effects"][0]["then"][0]["then"][0]["kind"],
        "counterStackObject"
    );
    assert_eq!(
        rule["effects"][1]["then"][0]["condition"]["where"],
        json!({ "kind": "colorContains", "value": "Red" })
    );
    assert_eq!(
        rule["effects"][1]["then"][0]["then"][0]["kind"],
        "destroyPermanent"
    );
}
