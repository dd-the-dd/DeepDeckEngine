use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};

const EXACT_ORACLE: &str = "+2: Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets -1/-0 until end of turn.\n−3: Return target instant or sorcery card from your graveyard to your hand. If it's a green card, add one mana of any color.\n−7: Draw cards equal to half the number of cards in your library, rounded up. You get an emblem with \"You have no maximum hand size.\"";

fn parse_loyalty_abilities(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Seasoned Scholar".to_string(),
        type_line: "Legendary Planeswalker — Test".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn tamiyo_loyalty_abilities_are_native_compositions_of_primitive_rules() {
    let truth: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/tamiyo-seasoned-scholar-loyalty.json"
    ))
    .expect("Tamiyo loyalty ground truth is valid JSON");
    assert_eq!(truth["context"]["oracleText"], EXACT_ORACLE);

    let parsed = parse_loyalty_abilities(EXACT_ORACLE);
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 3);
    for (ability, expected) in parsed.abilities.iter().zip(
        truth["abilities"]
            .as_array()
            .expect("ground-truth abilities"),
    ) {
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "canonical");
        assert!(
            ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
        );
        assert_eq!(ability.rule.as_ref(), Some(&expected["expectedRule"]));
    }
}

#[test]
fn another_planeswalker_reuses_the_protected_attack_event_and_stat_modifier() {
    let parsed = parse_loyalty_abilities(
        "−3: Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets -5/-0 until end of turn.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "canonical");
    let rule = ability.rule.as_ref().expect("primitive sibling rule");
    assert_eq!(rule["costs"][0]["amount"]["value"], -3);
    assert_eq!(rule["effects"][0]["effects"][0]["power"]["value"], -5);
    assert_eq!(rule["effects"][0]["effects"][0]["toughness"]["value"], 0);
}

/// An attacked opponent is the shared attack event's recipient, and `it` in
/// the created trigger names that trigger's own attacker (CR 603.7c).
#[test]
fn an_attacked_opponent_is_the_created_trigger_recipient() {
    let parsed = parse_loyalty_abilities(
        "+2: Until your next turn, whenever a creature attacks an opponent, it gets -1/-0 until end of turn.",
    );

    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "canonical", "{:#?}", ability.diagnostics);
    let rule = ability.rule.as_ref().expect("a created trigger");
    let created = &rule["effects"][0];
    assert_eq!(created["kind"], "installCreatedTrigger");
    assert_eq!(
        created["event"]["attacked"]["anyOf"][0]["player"]["kind"],
        "opponentsOf"
    );
    assert_eq!(
        created["effects"][0]["object"]["kind"],
        "triggeringPermanent"
    );
}
