use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse_enchantment(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Enchantment — Plan".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn controlled_creature_taps_compose_loot_counter_and_reflexive_return() {
    let parsed = parse_enchantment(
        "Rewrite History",
        "Whenever one or more creatures you control become tapped, draw a card, then discard a card and put a plan counter on this enchantment.\nWhen the fourth plan counter is put on this enchantment, sacrifice it. When you do, return up to two target instant and/or sorcery cards from your graveyard to your hand.",
    );

    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    assert_eq!(parsed.abilities.len(), 2);

    let tap_rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the controlled-tap trigger should be canonical");
    assert_eq!(tap_rule["event"]["kind"], "controlledPermanentsTapped");
    assert_eq!(tap_rule["event"]["where"]["kind"], "cardTypeContains");
    assert_eq!(tap_rule["event"]["where"]["value"], "Creature");
    assert_eq!(tap_rule["effects"][0]["kind"], "drawCards");
    assert_eq!(tap_rule["effects"][1]["kind"], "discardCards");
    assert_eq!(tap_rule["effects"][2]["kind"], "putCounters");
    assert_eq!(tap_rule["effects"][2]["counter"], "plan");

    let counter_rule = parsed.abilities[1]
        .rule
        .as_ref()
        .expect("the ordinal-counter trigger should be canonical");
    assert_eq!(counter_rule["event"]["kind"], "countersPlaced");
    assert_eq!(counter_rule["event"]["counter"], "plan");
    assert_eq!(counter_rule["event"]["resultingCount"]["value"], 4);
    assert_eq!(counter_rule["effects"][0]["kind"], "sacrificePermanent");
    assert_eq!(counter_rule["effects"][1]["kind"], "createReflexiveTrigger");
    let reflexive = &counter_rule["effects"][1]["ability"];
    assert_eq!(reflexive["declaration"]["decisions"][0]["minimum"], 0);
    assert_eq!(reflexive["declaration"]["decisions"][0]["maximum"], 2);
    assert_eq!(reflexive["effects"][0]["kind"], "moveCards");
}

#[test]
fn tap_subject_and_loot_counts_are_independent_axes() {
    let parsed = parse_enchantment(
        "Test Plan",
        "Whenever one or more artifacts you control become tapped, draw two cards, then discard a card and put a charge counter on this enchantment.",
    );

    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.as_ref().expect("sibling rule");
    assert_eq!(rule["event"]["where"]["value"], "Artifact");
    assert_eq!(rule["effects"][0]["count"]["value"], 2);
    assert_eq!(rule["effects"][1]["count"]["value"], 1);
    assert_eq!(rule["effects"][2]["counter"], "charge");
}

#[test]
fn untap_wording_is_not_accepted_as_a_tap_event() {
    let parsed = parse_enchantment(
        "Test Plan",
        "Whenever one or more creatures you control become untapped, draw a card, then discard a card and put a plan counter on this enchantment.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn enter_trigger_reuses_the_same_draw_then_discard_subgrammar() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Shuttle".to_string(),
        type_line: "Artifact — Spacecraft".to_string(),
        mana_cost: Some("{3}".to_string()),
        oracle_text: Some(
            "When this Spacecraft enters, draw two cards, then discard a card.".to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.as_ref().expect("entry trigger");
    assert_eq!(rule["event"]["kind"], "enterBattlefield");
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
    assert_eq!(rule["effects"][0]["count"]["value"], 2);
    assert_eq!(rule["effects"][1]["kind"], "discardCards");
    assert_eq!(rule["effects"][1]["count"]["value"], 1);
}

#[test]
fn activated_ability_reuses_loot_sequence_before_a_source_counter() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Scholar".to_string(),
        type_line: "Creature — Human".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(
            "{T}: Draw a card, then discard a card. Put a quest counter on Test Scholar."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("activated loot rule");
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
    assert_eq!(rule["effects"][1]["kind"], "discardCards");
    assert_eq!(rule["effects"][2]["kind"], "putCounters");
    assert_eq!(rule["effects"][2]["counter"], "quest");
}
