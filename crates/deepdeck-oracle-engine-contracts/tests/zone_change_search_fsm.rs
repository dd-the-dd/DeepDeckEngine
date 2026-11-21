use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

fn assert_executable(oracle_text: &str, expected_zone_change: &str) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Zone Change Search Test".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("native grammar should emit a rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], expected_zone_change);
    assert_eq!(rule["effects"][2]["kind"], "optionalEffects");
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
}

#[test]
fn destroy_then_optional_search_is_primitive_and_executable() {
    assert_executable(
        "Destroy target creature or planeswalker. Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle.",
        "destroyPermanent",
    );
}

#[test]
fn exile_then_optional_search_reuses_the_same_executable_contract() {
    assert_executable(
        "Exile target creature. Its controller may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
        "exilePermanent",
    );
}
