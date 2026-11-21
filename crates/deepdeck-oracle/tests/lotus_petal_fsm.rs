use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const LOTUS_PETAL_ORACLE: &str = "{T}, Sacrifice this artifact: Add one mana of any color.";

#[test]
fn lotus_petal_composes_native_cost_and_mana_automata() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Lotus Petal".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(LOTUS_PETAL_ORACLE.to_string()),
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
                { "kind": "tap", "object": { "kind": "self" } },
                {
                    "kind": "sacrificePermanent",
                    "permanent": { "kind": "self" },
                },
            ],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "mana": { "kind": "chooseColor", "amount": 1 },
            }],
        }))
    );
}
