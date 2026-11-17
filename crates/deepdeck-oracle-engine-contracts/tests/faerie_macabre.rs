use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn discard_self_to_exile_up_to_two_graveyard_cards_is_native_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Faerie Macabre".to_string(),
        type_line: "Creature — Faerie Rogue".to_string(),
        mana_cost: Some("{1}{B}{B}".to_string()),
        oracle_text: Some(
            "Flying\nDiscard this card: Exile up to two target cards from graveyards.".to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability.rule.as_ref().is_some_and(rule_is_executable)
    }));
    let rule = parsed.abilities[1].rule.as_ref().unwrap();
    assert_eq!(rule["activationZone"], "hand");
    assert_eq!(rule["costs"][0]["kind"], "discardCard");
    assert_eq!(rule["effects"][0]["kind"], "exileTargetCards");
}
