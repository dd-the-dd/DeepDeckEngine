use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn targeted_hand_disruption_with_life_loss_is_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Thoughtseize".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{B}".to_string()),
        oracle_text: Some(
            "Target player reveals their hand. You choose a nonland card from it. That player discards that card. You lose 2 life."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability
        .rule
        .as_ref()
        .expect("Thoughtseize emits primitive IR");
    assert!(rule_is_executable(rule));
    assert_eq!(
        rule["effects"]
            .as_array()
            .expect("spell effects")
            .iter()
            .map(|effect| effect["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["revealHand", "chooseCards", "discardCards", "loseLife"]
    );
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
}
