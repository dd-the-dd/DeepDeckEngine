use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parsed_rule(name: &str, oracle_text: &str) -> serde_json::Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    parsed.abilities[0]
        .rule
        .clone()
        .expect("local effect sequence lowers to canonical IR")
}

#[test]
fn damage_then_life_primitives_are_executable_without_a_composite_operation() {
    let rule = parsed_rule(
        "Lightning Helix",
        "Lightning Helix deals 3 damage to any target and you gain 3 life.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(rule["effects"][0]["kind"], "dealDamage");
    assert_eq!(rule["effects"][1]["kind"], "gainLife");
    assert!(!rule.to_string().contains("dealAndGainLife"));
}

#[test]
fn variable_damage_player_target_and_word_life_are_executable() {
    let rule = parsed_rule(
        "Example Source",
        "This spell deals X damage to target player then you gain three life.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseNumber");
    assert_eq!(rule["declaration"]["decisions"][1]["kind"], "chooseTargets");
    assert_eq!(rule["effects"][0]["amount"]["kind"], "decisionResult");
    assert_eq!(rule["effects"][1]["amount"]["value"], 3);
}
