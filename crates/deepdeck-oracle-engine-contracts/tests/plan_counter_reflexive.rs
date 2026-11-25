use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parsed_rules(oracle_text: &str) -> Vec<serde_json::Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Plan".to_string(),
        type_line: "Enchantment — Plan".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical rule"))
        .collect()
}

#[test]
fn controlled_tap_and_ordinal_reflexive_rules_are_executable() {
    let rules = parsed_rules(
        "Whenever one or more creatures you control become tapped, draw a card, then discard a card and put a plan counter on this enchantment.\nWhen the fourth plan counter is put on this enchantment, sacrifice it. When you do, return up to two target instant and/or sorcery cards from your graveyard to your hand.",
    );

    assert_eq!(rules.len(), 2);
    assert!(rules.iter().all(rule_is_executable));
    let serialized = serde_json::to_string(&rules).expect("rules serialize");
    assert!(!serialized.contains("drawThenDiscard"));
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn filtered_group_tap_event_is_not_locked_to_creatures() {
    let rules = parsed_rules(
        "Whenever one or more artifacts you control become tapped, draw two cards, then discard a card and put a charge counter on this enchantment.",
    );

    assert_eq!(rules[0]["event"]["kind"], "controlledPermanentsTapped");
    assert_eq!(rules[0]["event"]["where"]["value"], "Artifact");
    assert!(rule_is_executable(&rules[0]));
}
