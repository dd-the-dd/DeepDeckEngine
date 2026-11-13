use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "Choose one \u{2014}\n\u{2022} Boros Charm deals 4 damage to target player or planeswalker.\n\u{2022} Permanents you control gain indestructible until end of turn.\n\u{2022} Target creature gains double strike until end of turn.";

#[test]
fn boros_charm_native_modal_primitives_are_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Boros Charm".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("parse result serializes")
    );
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native modal rule");
    assert!(rule_is_executable(rule));
    let serialized = serde_json::to_string(rule).expect("rule serializes");
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("borosCharm"));
    assert_eq!(
        rule["effects"]
            .as_array()
            .expect("modal effects")
            .iter()
            .map(|mode| mode["then"].as_array().map_or(0, Vec::len))
            .sum::<usize>(),
        3
    );
}
