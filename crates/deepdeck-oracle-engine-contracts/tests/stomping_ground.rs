use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn both_stomping_ground_native_rules_are_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Stomping Ground".to_string(),
        type_line: "Land \u{2014} Mountain Forest".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(
            "({T}: Add {R} or {G}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped."
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
