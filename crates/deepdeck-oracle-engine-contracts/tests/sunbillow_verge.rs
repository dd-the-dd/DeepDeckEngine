use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn both_sunbillow_verge_mana_abilities_are_native_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Sunbillow Verge".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(
            "{T}: Add {W}.\n{T}: Add {R}. Activate only if you control a Mountain or a Plains."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability.rule.as_ref().is_some_and(rule_is_executable)
    }));
}
