use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn tapped_basic_land_search_is_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Rampant Growth".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(
            "Search your library for a basic land card, put that card onto the battlefield tapped, then shuffle."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("primitive rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "chooseCards");
    assert_eq!(rule["effects"][0]["candidates"]["where"]["kind"], "and");
    assert_eq!(rule["effects"][1]["kind"], "moveCards");
    assert_eq!(rule["effects"][1]["to"]["tapped"], true);
    assert_eq!(rule["effects"][2]["kind"], "shuffleZone");
    assert_eq!(rule["effects"][3]["kind"], "recordLibrarySearch");
}
