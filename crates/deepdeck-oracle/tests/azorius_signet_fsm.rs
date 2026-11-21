use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

#[test]
fn azorius_signet_is_native_without_unknowns_and_has_exact_primitive_ir() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Azorius Signet".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some("{1}, {T}: Add {W}{U}.".to_string()),
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
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "payMana", "manaCost": "{1}" },
                { "kind": "tap", "object": { "kind": "self" } },
            ],
            "effects": [{
                "kind": "addMana",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": "{W}{U}",
            }],
        }))
    );
}
