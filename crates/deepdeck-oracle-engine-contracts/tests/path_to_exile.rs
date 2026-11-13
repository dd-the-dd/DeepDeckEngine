use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, parse_oracle_card},
};

#[test]
fn path_to_exile_primitive_sequence_is_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Path to Exile".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(
            "Exile target creature. Its controller may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("Path to Exile should produce canonical primitive IR");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], "exilePermanent");
    assert_eq!(rule["effects"][2]["kind"], "optionalEffects");
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
}
