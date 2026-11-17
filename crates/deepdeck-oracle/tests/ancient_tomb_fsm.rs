use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const ANCIENT_TOMB_ORACLE: &str = "{T}: Add {C}{C}. This land deals 2 damage to you.";

#[test]
fn ancient_tomb_composes_fixed_mana_with_targetless_controller_damage() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Ancient Tomb".to_string(),
        type_line: "Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(ANCIENT_TOMB_ORACLE.to_string()),
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
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [
                {
                    "kind": "addMana",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "mana": "{C}{C}",
                },
                {
                    "kind": "dealDamage",
                    "source": { "kind": "self" },
                    "amount": { "kind": "integer", "value": 2 },
                    "recipient": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
            ],
        }))
    );
    assert!(
        !ability
            .rule
            .as_ref()
            .unwrap()
            .to_string()
            .contains("resolve")
    );
}
