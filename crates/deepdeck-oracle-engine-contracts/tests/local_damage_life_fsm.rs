use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};
use serde_json::Value;

fn parse_damage_life_rule(card_name: &str, oracle_text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("local-damage-life-contract-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("contract parser thread starts")
        .join()
        .expect("contract parser thread completes");

    assert_eq!(parsed.status, "canonical");
    parsed.abilities[0]
        .rule
        .clone()
        .expect("the local damage-and-life automata emit a canonical rule")
}

fn assert_independent_executable_primitives(rule: &Value) {
    assert!(rule_is_executable(rule));
    let effects = rule["effects"].as_array().expect("ordered effect sequence");
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0]["kind"], "dealDamage");
    assert_eq!(effects[1]["kind"], "gainLife");

    let serialized = rule.to_string();
    assert!(!serialized.contains("dealAndGainLife"));
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn lightning_helix_is_an_executable_sequence_of_two_primitives() {
    let rule = parse_damage_life_rule(
        "Lightning Helix",
        "Lightning Helix deals 3 damage to any target and you gain 3 life.",
    );

    assert_independent_executable_primitives(&rule);
    assert_eq!(rule["effects"][0]["amount"]["value"], 3);
    assert_eq!(rule["effects"][1]["amount"]["value"], 3);
    assert_eq!(
        rule["declaration"]["decisions"][0]["id"],
        "targetDamageable"
    );
}

#[test]
fn this_spell_and_target_player_reuse_the_same_primitive_contract() {
    let rule = parse_damage_life_rule(
        "Example Source",
        "This spell deals 4 damage to target player then you gain two life.",
    );

    assert_independent_executable_primitives(&rule);
    assert_eq!(rule["effects"][0]["amount"]["value"], 4);
    assert_eq!(rule["effects"][1]["amount"]["value"], 2);
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPlayer");
}
