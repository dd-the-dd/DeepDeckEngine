use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_land(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Early Land".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn early_turn_land_entry_compiles_to_turn_count_and_comparison_primitives() {
    let parsed = parse_land(
        "This land enters tapped unless it's your first, second, or third turn of the game.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the early-turn land replacement should parse");
    assert_eq!(
        rule,
        &json!({
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
                            "player": {
                                "kind": "controllerOf",
                                "object": { "kind": "self" },
                            },
                        },
                        {
                            "kind": "compare",
                            "operator": "<=",
                            "left": {
                                "kind": "countTurnsTaken",
                                "player": {
                                    "kind": "controllerOf",
                                    "object": { "kind": "self" },
                                },
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
        })
    );
}

#[test]
fn early_turn_entry_pattern_accepts_other_consecutive_thresholds() {
    let parsed =
        parse_land("This land enters tapped unless it's your first or second turn of the game.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the sibling early-turn threshold should parse");
    assert_eq!(
        rule["condition"]["operand"]["operands"][1]["left"]["kind"],
        "countTurnsTaken"
    );
    assert_eq!(
        rule["condition"]["operand"]["operands"][1]["right"]["value"],
        2
    );
}

#[test]
fn nonconsecutive_turn_list_is_not_approximated_as_a_threshold() {
    let parsed =
        parse_land("This land enters tapped unless it's your first or third turn of the game.");

    assert_eq!(parsed.status, "unsupported");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    let diagnostic = ability
        .diagnostics
        .first()
        .expect("the committed early-turn automaton should explain the gap");
    assert_eq!(diagnostic.automaton.as_deref(), Some("early-turn-entry"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("replacement.early-turn-entry.next-ordinal")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("third"));
}
