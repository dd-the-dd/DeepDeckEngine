use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

fn parse_rule(name: &str, mana_cost: &str, oracle_text: &str) -> serde_json::Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("plain-damage-contract-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("contract parser thread starts")
        .join()
        .expect("contract parser thread completes");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    parsed.abilities[0]
        .rule
        .clone()
        .expect("native plain damage emits a canonical rule")
}

#[test]
fn lightning_bolt_is_an_executable_atomic_damage_rule() {
    let rule = parse_rule(
        "Lightning Bolt",
        "{R}",
        "Lightning Bolt deals 3 damage to any target.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(1));
    assert_eq!(rule["effects"][0]["kind"], "dealDamage");
    assert_eq!(rule["effects"][0]["amount"]["value"], 3);
    let serialized = rule.to_string();
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn variable_damage_and_typed_union_share_the_same_executable_contract() {
    let rule = parse_rule(
        "Test Spark",
        "{X}{R}",
        "Test Spark deals X damage to target creature or planeswalker.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(rule["effects"][0]["amount"]["kind"], "decisionResult");
    assert_eq!(rule["declaration"]["decisions"][1]["id"], "targetPermanent");
}
