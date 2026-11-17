use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

const STOMPING_GROUND_ORACLE: &str = "({T}: Add {R} or {G}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.";

#[test]
fn stomping_ground_is_fully_native_without_unknowns_and_has_exact_primitive_ir() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Stomping Ground".to_string(),
        type_line: "Land \u{2014} Mountain Forest".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(STOMPING_GROUND_ORACLE.to_string()),
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
        parsed
            .abilities
            .iter()
            .map(|ability| ability.rule.clone().expect("native ability must lower"))
            .collect::<Vec<_>>(),
        vec![
            json!({
                "kind": "manaAbility",
                "source": { "kind": "self" },
                "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
                "effects": [{
                    "kind": "addMana",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "mana": { "kind": "chooseOne", "options": ["{R}", "{G}"] },
                }],
            }),
            json!({
                "kind": "replacementEffect",
                "source": { "kind": "self" },
                "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
                "decisions": [{
                    "id": "optionalLifePayment",
                    "kind": "chooseWhetherToPay",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "cost": {
                        "kind": "payLife",
                        "amount": { "kind": "integer", "value": 2 },
                    },
                }],
                "replacement": [{
                    "kind": "conditional",
                    "condition": {
                        "kind": "not",
                        "operand": {
                            "kind": "costWasPaid",
                            "decisionId": "optionalLifePayment",
                        },
                    },
                    "then": [{
                        "kind": "setEnteringState",
                        "object": { "kind": "self" },
                        "tapped": true,
                    }],
                }],
            }),
        ]
    );
}
