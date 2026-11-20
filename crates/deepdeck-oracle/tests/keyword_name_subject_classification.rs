use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};

fn parse(card_name: &str, oracle_text: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{4}{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn current_card_name_at_token_zero_outweighs_its_keyword_homonym() {
    let parsed = parse(
        "Awaken the Inferno",
        "Awaken the Inferno deals 6 damage to target creature or planeswalker an opponent controls. Put a +1/+1 counter on up to one target creature you control.\nBasic landcycling {2} ({2}, Discard this card: Search your library for a basic land card, reveal it, put it into your hand, then shuffle.)",
    );

    // Basic landcycling is now a native typecycling keyword, so the whole
    // card is canonical.
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.ability_type, "spellAbility");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "canonical");
    let rule = ability.rule.as_ref().expect("spell rule");
    // The current name is the damage source, never the `awaken` keyword.
    assert_eq!(rule["effects"][0]["kind"], "dealDamage");
    assert_eq!(
        rule["effects"][0]["source"],
        serde_json::json!({ "kind": "self" })
    );
    assert_eq!(rule["effects"][0]["amount"]["value"], 6);
    assert_eq!(rule["effects"][1]["kind"], "putCounters");
    assert!(!rule.to_string().to_ascii_lowercase().contains("awaken"));

    let landcycling = &parsed.abilities[1];
    assert_eq!(landcycling.status, "canonical");
    assert_eq!(landcycling.recognizer, RecognizerProvenance::Native);
    let keyword = &landcycling.rule.as_ref().expect("landcycling rule")["ability"];
    assert_eq!(keyword["kind"], "typecycling");
    assert_eq!(keyword["activationZone"], "hand");
    assert_eq!(
        keyword["cost"],
        serde_json::json!({ "kind": "payMana", "manaCost": "{2}" })
    );
    assert_eq!(
        keyword["where"],
        serde_json::json!({
            "kind": "and",
            "operands": [
                { "kind": "typeLineContains", "value": "Basic" },
                { "kind": "cardTypeContains", "value": "Land" },
            ],
        })
    );
}

#[test]
fn another_current_name_beginning_with_awaken_is_a_damage_subject() {
    let parsed = parse(
        "Awaken the Furnace",
        "Awaken the Furnace deals 2 damage to target creature.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.ability_type, "spellAbility");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][0]["kind"],
        "dealDamage"
    );
}

#[test]
fn unimplemented_awaken_keyword_is_explicitly_unsupported_without_a_name_link() {
    let parsed = parse(
        "Test Spell",
        "Awaken 4—{4}{W} (If you cast this spell for {4}{W}, also put four +1/+1 counters on target land you control and it becomes a 0/0 Elemental creature with haste. It's still a land.)",
    );

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.ability_type, "keywordAbility");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(ability.diagnostics[0].code, "oracle_machine_failure");
}
