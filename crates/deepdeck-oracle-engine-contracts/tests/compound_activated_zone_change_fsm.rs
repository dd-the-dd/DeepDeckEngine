use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn compound_tap_sacrifice_activation_is_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Utility Land".to_string(),
        type_line: "Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some("{T}, Sacrifice this land: Destroy target nonbasic land.".to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("native grammar emits a rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["kind"], "activatedAbility");
    assert_eq!(rule["costs"][0]["kind"], "tap");
    assert_eq!(rule["costs"][1]["kind"], "sacrificePermanent");
    assert_eq!(rule["effects"][0]["kind"], "destroyPermanent");
}
