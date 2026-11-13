use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "Counter target spell. Its controller may draw up to two cards at the beginning of the next turn's upkeep.\nYou draw a card at the beginning of the next turn's upkeep.";

#[test]
fn counter_and_delayed_draw_rules_are_native_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Arcane Denial".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    for ability in &parsed.abilities {
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert!(rule_is_executable(
            ability.rule.as_ref().expect("native primitive rule")
        ));
    }
    let first = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("counter and target-controller delayed draw");
    assert_eq!(first["effects"][0]["kind"], "bind");
    assert_eq!(first["effects"][1]["kind"], "counterSpell");
    assert_eq!(first["effects"][2]["kind"], "installDelayedStepTrigger");
    assert_eq!(
        first["effects"][2]["captureBindings"][0],
        "counteredSpellController"
    );
    assert_eq!(first["effects"][2]["effects"][0]["kind"], "chooseNumber");
    assert_eq!(first["effects"][2]["effects"][1]["kind"], "drawCards");
    assert_eq!(
        parsed.abilities[1].rule.as_ref().expect("caster draw")["effects"][0]["kind"],
        "installDelayedStepTrigger"
    );
}
