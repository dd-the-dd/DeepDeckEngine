use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "Counter target enchantment, instant, or sorcery spell. Its controller creates a 2/2 blue Bird creature token with flying.";

#[test]
fn swan_song_native_primitives_satisfy_the_engine_contract() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Swan Song".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native primitive rule");
    assert!(rule_is_executable(rule));
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], "counterSpell");
    assert_eq!(rule["effects"][2]["kind"], "createTokens");
    assert_eq!(
        rule["effects"][2]["controller"]["id"],
        "counteredSpellController"
    );
    assert_eq!(
        rule["effects"][2]["token"]["abilities"][0]["kind"],
        "flying"
    );
}
