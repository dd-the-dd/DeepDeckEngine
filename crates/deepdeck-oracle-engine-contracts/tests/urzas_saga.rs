use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "(As this Saga enters and after your draw step, add a lore counter. Sacrifice after III.)\nI — This Saga gains \"{T}: Add {C}.\"\nII — This Saga gains \"{2}, {T}: Create a 0/0 colorless Construct artifact creature token with 'This token gets +1/+1 for each artifact you control.'\"\nIII — Search your library for an artifact card with mana cost {0} or {1}, put it onto the battlefield, then shuffle.";

#[test]
fn every_urzas_saga_rule_is_native_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Urza's Saga".to_string(),
        type_line: "Enchantment Land — Urza's Saga".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("saga".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 4);
    for ability in &parsed.abilities {
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        let rule = ability.rule.as_ref().expect("canonical primitive rule");
        assert!(rule_is_executable(rule), "non-executable rule: {rule}");
        assert!(!rule.to_string().contains("searchLibrary"));
        assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
    }
}
