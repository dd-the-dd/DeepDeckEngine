use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn opponent_controlled_target_and_optional_search_are_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Assassin's Trophy".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B}{G}".to_string()),
        oracle_text: Some(
            "Destroy target permanent an opponent controls. Its controller may search their library for a basic land card, put it onto the battlefield, then shuffle."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("primitive rule");
    assert!(rule_is_executable(rule));
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["controller"]["kind"],
        "opponentsOf"
    );
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][2]["kind"], "optionalEffects");
}
