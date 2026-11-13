use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const EXACT_ORACLE: &str =
    "{T}: Add {C}.\n{T}: Add {W} or {B}. This artifact deals 1 damage to you.";

#[test]
fn talisman_of_hierarchy_is_fully_native_and_primitive() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Talisman of Hierarchy".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(EXACT_ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["mana"],
        "{C}"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{
                "kind": "tap",
                "object": { "kind": "self" },
            }],
            "effects": [
                {
                    "kind": "addMana",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "mana": {
                        "kind": "chooseOne",
                        "options": ["{W}", "{B}"],
                    },
                },
                {
                    "kind": "dealDamage",
                    "source": { "kind": "self" },
                    "amount": { "kind": "integer", "value": 1 },
                    "recipient": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                },
            ],
        }))
    );
    assert!(parsed.abilities.iter().all(|ability| {
        !ability
            .rule
            .as_ref()
            .is_some_and(|rule| rule.to_string().contains("resolveSpellInstruction"))
    }));
}
