use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parse_rule(card_name: &str, mana_cost: &str, oracle_text: &str) -> serde_json::Value {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("ability-loss-destroy-contract-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("contract test thread starts")
        .join()
        .expect("contract test thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed.abilities[0]
        .rule
        .clone()
        .expect("the sequence should emit a canonical rule")
}

#[test]
fn variable_threshold_sequence_is_executable_and_primitive() {
    let rule = parse_rule(
        "Day of Black Sun",
        "{X}{B}{B}",
        "Each creature with mana value X or less loses all abilities until end of turn. Destroy those creatures.",
    );

    assert!(rule_is_executable(&rule));
    let serialized = rule.to_string();
    assert!(!serialized.contains("dayOfBlackSun"));
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn literal_threshold_sibling_uses_the_same_executable_contract() {
    let rule = parse_rule(
        "Test Eclipse",
        "{2}{B}",
        "Each artifact with mana value three or less loses all abilities until end of turn. Destroy those artifacts.",
    );

    assert!(rule_is_executable(&rule));
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], "forEachObject");
    assert_eq!(rule["effects"][2]["kind"], "destroyPermanent");
}
