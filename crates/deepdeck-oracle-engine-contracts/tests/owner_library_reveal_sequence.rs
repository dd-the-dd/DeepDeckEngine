use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parse_rule(oracle_text: &str) -> serde_json::Value {
    let request = OracleCardParseRequest {
        card_name: "Test Owner Reveal".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("owner-library-contract-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("contract test thread starts")
        .join()
        .expect("contract test thread completes");
    assert_eq!(parsed.status, "canonical");
    parsed.abilities[0]
        .rule
        .clone()
        .expect("owner-library sequence should emit a rule")
}

#[test]
fn exact_owner_library_reveal_sequence_is_executable_and_primitive() {
    let rule = parse_rule(
        "The owner of target permanent shuffles it into their library, then reveals the top card of their library. If it's a permanent card, they put it onto the battlefield.",
    );

    assert!(rule_is_executable(&rule));
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
    assert!(!rule.to_string().contains("chaosWarp"));
}

#[test]
fn independently_filtered_sibling_is_executable() {
    let rule = parse_rule(
        "The owner of target artifact shuffles it into their library, then reveals the top card of their library. If it's a creature card, they put that card onto the battlefield.",
    );

    assert!(rule_is_executable(&rule));
}
