use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn tap_sacrifice_and_color_choice_are_executable_primitives() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Lotus Petal".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some("{T}, Sacrifice this artifact: Add one mana of any color.".to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("primitive rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["costs"][0]["kind"], "tap");
    assert_eq!(rule["costs"][1]["kind"], "sacrificePermanent");
    assert_eq!(rule["effects"][0]["kind"], "addMana");
    assert_eq!(rule["effects"][0]["mana"]["kind"], "chooseColor");
}
