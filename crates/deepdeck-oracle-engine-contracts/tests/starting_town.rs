use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};
use serde_json::json;

#[test]
fn early_turn_entry_replacement_is_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Early Land".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(
            "This land enters tapped unless it's your first, second, or third turn of the game."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the replacement should produce canonical IR");
    assert!(rule_is_executable(rule));
    assert_eq!(
        rule["condition"]["operand"]["operands"][1]["left"]["kind"],
        "countTurnsTaken"
    );
}

#[test]
fn turn_count_accepts_a_generic_singular_player_reference() {
    let rule = json!({
        "kind": "spellAbility",
        "effects": [{
            "kind": "gainLife",
            "player": { "kind": "abilityController" },
            "amount": {
                "kind": "countTurnsTaken",
                "player": {
                    "kind": "ownerOf",
                    "object": { "kind": "self" }
                }
            }
        }]
    });

    assert!(rule_is_executable(&rule));
}
