use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn dismember_native_temporary_modifier_is_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Dismember".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B/P}{B/P}".to_string()),
        oracle_text: Some(
            "({B/P} can be paid with either {B} or 2 life.)\nTarget creature gets -5/-5 until end of turn."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("parse result serializes")
    );
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    for rule in parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.as_ref().expect("native rule"))
    {
        assert!(rule_is_executable(rule));
        assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
    }
    let effect_rule = parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .find(|rule| rule["kind"] == "spellAbility")
        .expect("spell effect rule");
    assert_eq!(effect_rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(effect_rule["effects"][0]["power"]["value"], -5);
    assert_eq!(effect_rule["effects"][0]["toughness"]["value"], -5);
}
