use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parse_rules(oracle_text: &str) -> Vec<serde_json::Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Graveyard Seal".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical rule"))
        .collect()
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Rest in Peace; see docs/coverage-targets.md"
)]
#[test]
fn global_card_and_token_replacement_is_executable() {
    let rules = parse_rules(
        "When this enchantment enters, exile all graveyards.\nIf a card or token would be put into a graveyard from anywhere, exile it instead.",
    );

    assert!(rules.iter().all(rule_is_executable));
    assert!(
        rules
            .iter()
            .all(|rule| !rule.to_string().contains("resolve"))
    );
}

#[test]
fn scoped_card_only_replacement_is_executable() {
    let rules = parse_rules(
        "If a card would be put into an opponent's graveyard from anywhere, exile it instead.",
    );

    assert!(rules.iter().all(rule_is_executable));
}
