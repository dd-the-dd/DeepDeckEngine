use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn simple_typed_counter_is_native_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Negate".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some("Counter target noncreature spell.".to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("native counter grammar emits primitive IR");
    assert!(rule_is_executable(rule));
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["kind"],
        "spells"
    );
    assert_eq!(rule["effects"][0]["kind"], "counterSpell");
}
