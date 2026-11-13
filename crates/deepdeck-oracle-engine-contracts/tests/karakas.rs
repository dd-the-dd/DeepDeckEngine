use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn karakas_native_abilities_are_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Karakas".to_string(),
        type_line: "Legendary Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(
            "{T}: Add {W}.\n{T}: Return target legendary creature to its owner's hand.".to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let mana = parsed.abilities[0].rule.as_ref().expect("mana rule");
    let owner_hand = parsed.abilities[1].rule.as_ref().expect("owner-hand rule");
    assert!(rule_is_executable(mana));
    assert!(rule_is_executable(owner_hand));
    assert_eq!(mana["kind"], "manaAbility");
    assert_eq!(owner_hand["kind"], "activatedAbility");
    assert_eq!(owner_hand["costs"][0]["kind"], "tap");
    assert_eq!(
        owner_hand["declaration"]["decisions"][0]["candidates"]["where"]["operands"][0]["kind"],
        "isLegendary"
    );
    assert_eq!(owner_hand["effects"][0]["kind"], "returnToOwnersHand");
}
