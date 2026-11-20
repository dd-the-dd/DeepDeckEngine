use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardFace, OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn source_transform_action_is_native_primitive_and_executable() {
    // A transform layout has exactly two printed parts (the card face
    // factory rejects a faceless transform request); the Incubator token
    // transforms into a Phyrexian artifact creature without rules text.
    let face = |id: &str, name: &str, type_line: &str, oracle_text: &str| OracleCardFace {
        id: id.to_string(),
        name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: oracle_text.to_string(),
        power: None,
        toughness: None,
        loyalty: None,
    };
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Incubator // Phyrexian Token".to_string(),
        type_line: "Token Artifact — Incubator // Token Artifact Creature — Phyrexian".to_string(),
        mana_cost: None,
        oracle_text: None,
        layout: Some("transform".to_string()),
        faces: vec![
            face(
                "front",
                "Incubator",
                "Token Artifact — Incubator",
                "{2}: Transform this token.",
            ),
            face(
                "back",
                "Phyrexian Token",
                "Token Artifact Creature — Phyrexian",
                "",
            ),
        ],
    });

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native transform rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "transformPermanent");
}
