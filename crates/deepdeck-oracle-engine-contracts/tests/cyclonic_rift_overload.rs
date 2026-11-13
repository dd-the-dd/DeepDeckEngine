use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "Return target nonland permanent you don't control to its owner's hand.\nOverload {6}{U} (You may cast this spell for its overload cost. If you do, change \"target\" in its text to \"each.\")";

#[test]
fn cyclonic_rift_compiles_only_to_executable_primitives() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Cyclonic Rift".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    for ability in &parsed.abilities {
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        let rule = ability.rule.as_ref().expect("native ability emits a rule");
        assert!(rule_is_executable(rule), "unexecutable rule: {rule}");
        assert!(!rule.to_string().contains("compileDirective"));
        assert!(!rule.to_string().contains("returnPermanentsToOwnersHands"));
        assert!(!rule.to_string().contains("resolveSpellInstruction"));
    }

    let spell = parsed.abilities[0].rule.as_ref().expect("spell rule");
    assert_eq!(spell["effects"][0]["kind"], "conditionalEffect");
    assert_eq!(
        spell["effects"][0]["then"][0]["object"]["kind"],
        "eachPermanent"
    );
    assert_eq!(
        spell["effects"][0]["else"][0]["object"]["kind"],
        "chosenTarget"
    );
}
