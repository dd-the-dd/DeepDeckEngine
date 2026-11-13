use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const ORACLE: &str =
    "{T}: Add {W}.\n{T}: Add {R}. Activate only if you control a Mountain or a Plains.";

#[test]
fn sunbillow_verge_is_two_fully_native_primitive_mana_abilities() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Sunbillow Verge".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.status == "canonical"
            && ability.recognizer == RecognizerProvenance::Native
            && ability.diagnostics.is_empty()
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed
            .abilities
            .iter()
            .map(|ability| ability.rule.as_ref().expect("native ability emits IR"))
            .collect::<Vec<_>>(),
        vec![
            &json!({
                "kind": "manaAbility",
                "source": { "kind": "self" },
                "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
                "effects": [{
                    "kind": "addMana",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "mana": "{W}",
                }],
            }),
            &json!({
                "kind": "manaAbility",
                "source": { "kind": "self" },
                "activationCondition": {
                    "kind": "controlsPermanent",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "where": {
                        "kind": "or",
                        "operands": [
                            { "kind": "subtypeContains", "value": "Mountain" },
                            { "kind": "subtypeContains", "value": "Plains" },
                        ],
                    },
                },
                "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
                "effects": [{
                    "kind": "addMana",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "mana": "{R}",
                }],
            }),
        ]
    );
}
