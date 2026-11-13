use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "Creature spells you control can't be countered.\nNontoken creatures you control have riot. (They enter with your choice of a +1/+1 counter or haste.)";

#[test]
fn rhythm_rules_are_native_filtered_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Rhythm of the Wild".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{R}{G}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    for ability in &parsed.abilities {
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        let rule = ability.rule.as_ref().expect("canonical primitive rule");
        assert!(rule_is_executable(rule), "non-executable rule: {rule}");
        assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
    }
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["modifiers"][0]["objects"]["where"]["value"],
        "Creature"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["modifiers"][0]["kind"],
        "grantKeyword"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["modifiers"][0]["keyword"],
        "riot"
    );
    let all_ir = parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .map(ToString::to_string)
        .collect::<String>();
    assert!(!all_ir.contains("grantRiot"));
    assert!(!all_ir.contains("applyRiotChoice"));
}
