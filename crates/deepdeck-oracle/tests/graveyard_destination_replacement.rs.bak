use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse_enchantment(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Graveyard Seal".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn cards_and_tokens_for_every_graveyard_compile_to_one_filtered_replacement() {
    let parsed = parse_enchantment(
        "When this enchantment enters, exile all graveyards.\nIf a card or token would be put into a graveyard from anywhere, exile it instead.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(
        parsed.abilities[0]
            .rule
            .as_ref()
            .expect("the entry trigger should parse")["effects"][0]["kind"],
        "exileAllGraveyards"
    );
    let modifier = &parsed.abilities[1]
        .rule
        .as_ref()
        .expect("the graveyard replacement should parse")["modifiers"][0];
    assert_eq!(modifier["kind"], "replaceGraveyardDestination");
    assert_eq!(modifier["graveyardOwners"]["kind"], "eachPlayer");
    assert!(modifier["where"].is_null());
    assert_eq!(modifier["from"]["kind"], "anyZone");
    assert_eq!(modifier["to"]["kind"], "exile");
}

#[test]
fn owner_scope_and_object_kind_are_independent_subgrammar_choices() {
    let opponent_cards = parse_enchantment(
        "If a card would be put into an opponent's graveyard from anywhere, exile it instead.",
    );
    let modifier = &opponent_cards.abilities[0]
        .rule
        .as_ref()
        .expect("opponent card replacement")["modifiers"][0];
    assert_eq!(modifier["graveyardOwners"]["kind"], "opponentsOf");
    assert_eq!(modifier["where"]["kind"], "not");
    assert_eq!(modifier["where"]["operand"]["kind"], "isToken");

    let your_tokens = parse_enchantment(
        "If a token would be put into your graveyard from anywhere, exile it instead.",
    );
    let modifier = &your_tokens.abilities[0]
        .rule
        .as_ref()
        .expect("controller token replacement")["modifiers"][0];
    assert_eq!(modifier["graveyardOwners"]["kind"], "controllerOf");
    assert_eq!(modifier["where"]["kind"], "isToken");
}

#[test]
fn unsupported_origin_is_not_widened_to_any_zone() {
    let parsed = parse_enchantment(
        "If a card would be put into a graveyard from your hand, exile it instead.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}
