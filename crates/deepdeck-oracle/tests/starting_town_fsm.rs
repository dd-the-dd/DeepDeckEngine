use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const STARTING_TOWN_ORACLE: &str = "This land enters tapped unless it's your first, second, or third turn of the game.\n{T}: Add {C}.\n{T}, Pay 1 life: Add one mana of any color.";

#[test]
fn starting_town_is_fully_native_and_primitive() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Starting Town".to_string(),
        type_line: "Land — Town".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(STARTING_TOWN_ORACLE.to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 3);
    let recognizers = parsed
        .abilities
        .iter()
        .map(|ability| ability.recognizer)
        .collect::<Vec<_>>();
    assert!(
        recognizers
            .iter()
            .all(|recognizer| *recognizer == RecognizerProvenance::Native),
        "{recognizers:?}"
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "replacementEffect",
            "source": { "kind": "self" },
            "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
            "condition": {
                "kind": "not",
                "operand": {
                    "kind": "and",
                    "operands": [
                        {
                            "kind": "duringControllerTurn",
                            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        },
                        {
                            "kind": "compare",
                            "operator": "<=",
                            "left": {
                                "kind": "countTurnsTaken",
                                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                            },
                            "right": { "kind": "integer", "value": 3 },
                        },
                    ],
                },
            },
            "replacement": [{
                "kind": "setEnteringState",
                "object": { "kind": "self" },
                "tapped": true,
            }],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [{
                "kind": "addMana",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": "{C}",
            }],
        }))
    );
    assert_eq!(
        parsed.abilities[2].rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "tap", "object": { "kind": "self" } },
                {
                    "kind": "payLife",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "amount": { "kind": "integer", "value": 1 },
                },
            ],
            "effects": [{
                "kind": "addMana",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": { "kind": "chooseColor", "amount": 1 },
            }],
        }))
    );
    assert!(parsed.abilities.iter().all(|ability| {
        let serialized = ability.rule.as_ref().unwrap().to_string();
        !serialized.contains("resolve") && !serialized.contains("operation")
    }));
}
