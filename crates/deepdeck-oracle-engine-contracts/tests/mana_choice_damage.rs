use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

#[test]
fn chosen_mana_followed_by_controller_damage_is_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Talisman of Dominance".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(
            "{T}: Add {C}.\n{T}: Add {U} or {B}. This artifact deals 1 damage to you.".to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    for ability in &parsed.abilities {
        let rule = ability.rule.as_ref().expect("each mana ability emits IR");
        assert!(rule_is_executable(rule));
        assert!(!rule.to_string().contains("resolveSpellInstruction"));
    }
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["effects"][0]["kind"],
        "addMana"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["effects"][1]["kind"],
        "dealDamage"
    );
}
