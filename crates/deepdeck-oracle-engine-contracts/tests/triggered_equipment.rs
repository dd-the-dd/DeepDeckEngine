use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

fn parse_equipment(card_name: &str, oracle_text: &str) -> serde_json::Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Artifact — Equipment".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    parsed.abilities[0]
        .rule
        .clone()
        .expect("the triggered ability should produce canonical IR")
}

#[test]
fn equipment_entry_followups_cross_the_parser_runtime_contract_as_primitives() {
    for (card_name, oracle_text, second_effect) in [
        (
            "Super Suit",
            "When this Equipment enters, attach it to target creature you control. Untap that creature.",
            "untapPermanent",
        ),
        (
            "Twin Blades",
            "When this Equipment enters, attach it to target creature you control. That creature gains double strike until end of turn.",
            "grantKeyword",
        ),
    ] {
        let rule = parse_equipment(card_name, oracle_text);

        assert_eq!(rule["effects"][0]["kind"], "attachPermanent");
        assert_eq!(rule["effects"][1]["kind"], second_effect);
        assert!(rule_is_executable(&rule));
        assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
        assert!(!rule.to_string().contains("twinBladesEnter"));
    }
}
