use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn source_transform_action_is_native_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Incubator Token".to_string(),
        type_line: "Token Artifact — Incubator".to_string(),
        mana_cost: None,
        oracle_text: Some("{2}: Transform this token.".to_string()),
        layout: Some("transform".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native transform rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "transformPermanent");
}
