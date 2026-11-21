use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn spirebluff_canal_rules_are_native_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Spirebluff Canal".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(
            "This land enters tapped unless you control two or fewer other lands.\n{T}: Add {U} or {R}."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 2);
    for ability in &parsed.abilities {
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert!(
            rule_is_executable(ability.rule.as_ref().expect("canonical primitive rule")),
            "{ability:#?}"
        );
    }
}
