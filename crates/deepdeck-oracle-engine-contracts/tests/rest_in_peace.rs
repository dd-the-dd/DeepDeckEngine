use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Rest in Peace; see docs/coverage-targets.md"
)]
#[test]
fn rest_in_peace_native_rules_are_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Rest in Peace".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(
            "When this enchantment enters, exile all graveyards.\nIf a card or token would be put into a graveyard from anywhere, exile it instead."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let rules = parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.as_ref().expect("native canonical rule"))
        .collect::<Vec<_>>();
    assert!(rules.iter().all(|rule| rule_is_executable(rule)));
    assert_eq!(rules[0]["effects"][0]["kind"], "exileAllGraveyards");
    assert_eq!(
        rules[1]["modifiers"][0]["kind"],
        "replaceGraveyardDestination"
    );
    assert!(
        rules
            .iter()
            .all(|rule| !rule.to_string().contains("resolve"))
    );
}
