use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn entering_creature_trigger_suppression_is_an_executable_primitive() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Torpor Orb".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some("Creatures entering don't cause abilities to trigger.".to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("primitive rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["modifiers"][0]["kind"], "suppressTriggerCreation");
    assert_eq!(
        rule["modifiers"][0]["causedBy"]["object"]["where"]["value"],
        "Creature"
    );
}
