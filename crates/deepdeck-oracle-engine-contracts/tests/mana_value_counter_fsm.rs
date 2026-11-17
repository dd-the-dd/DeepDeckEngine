use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn disdainful_stroke_is_native_primitive_and_executable() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/disdainful-stroke.json"
    ))
    .expect("Disdainful Stroke fixture is valid JSON");
    let context = &truth["context"];
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: context["name"].as_str().unwrap().to_string(),
        type_line: context["typeLine"].as_str().unwrap().to_string(),
        mana_cost: Some(context["manaCost"].as_str().unwrap().to_string()),
        oracle_text: Some(context["oracleText"].as_str().unwrap().to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("native counter grammar emits primitive IR");
    assert_eq!(rule, &truth["abilities"][0]["expectedRule"]);
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "counterSpell");
}
