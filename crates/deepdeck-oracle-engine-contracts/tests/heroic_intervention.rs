use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn controlled_temporary_keyword_grants_are_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Heroic Intervention".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(
            "Permanents you control gain hexproof and indestructible until end of turn."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("primitive rule");
    assert!(rule_is_executable(rule));
    let effects = rule["effects"].as_array().expect("keyword effects");
    assert_eq!(effects.len(), 2);
    assert!(
        effects
            .iter()
            .all(|effect| effect["kind"] == "grantKeyword")
    );
}
