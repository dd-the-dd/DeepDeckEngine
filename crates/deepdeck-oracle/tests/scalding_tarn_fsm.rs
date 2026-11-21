use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const SCALDING_TARN_ORACLE: &str = "{T}, Pay 1 life, Sacrifice this land: Search your library for an Island or Mountain card, put it onto the battlefield, then shuffle.";

#[test]
fn scalding_tarn_reuses_the_native_fetch_land_automaton() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Scalding Tarn".to_string(),
        type_line: "Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(SCALDING_TARN_ORACLE.to_string()),
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
    let rule = ability.rule.as_ref().expect("primitive fetch-land rule");
    assert_eq!(
        rule["costs"],
        json!([
            { "kind": "tap", "object": { "kind": "self" } },
            {
                "kind": "payLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 1 },
            },
            { "kind": "sacrificePermanent", "permanent": { "kind": "self" } },
        ])
    );
    assert_eq!(
        rule["effects"][0]["candidates"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "subtypeContains", "value": "Island" },
                { "kind": "subtypeContains", "value": "Mountain" },
            ],
        })
    );
    assert_eq!(rule["effects"][1]["kind"], "moveCards");
    assert_eq!(rule["effects"][1]["to"]["tapped"], false);
    assert_eq!(rule["effects"][2]["kind"], "shuffleZone");
    assert_eq!(rule["effects"][3]["kind"], "recordLibrarySearch");
    assert!(!rule.to_string().contains("resolve"));
}
