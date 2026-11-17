use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn swords_snapshot_sequence_is_executable_without_a_composite_effect() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Swords to Plowshares".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(
            "Exile target creature. Its controller gains life equal to its power.".to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("Swords should produce canonical primitive IR");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], "bind");
    assert_eq!(rule["effects"][1]["value"]["kind"], "maximum");
    assert_eq!(rule["effects"][2]["kind"], "exilePermanent");
    assert_eq!(rule["effects"][3]["kind"], "gainLife");
    assert!(
        !rule
            .to_string()
            .contains("exilePermanentAndControllerGainsPower")
    );
}
