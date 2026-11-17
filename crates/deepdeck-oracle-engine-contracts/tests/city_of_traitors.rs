use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "When you play another land, sacrifice this land.\n{T}: Add {C}{C}.";

#[test]
fn city_of_traitors_rules_are_native_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "City of Traitors".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    for ability in &parsed.abilities {
        let rule = ability.rule.as_ref().expect("native primitive rule");
        assert!(rule_is_executable(rule), "unexecutable rule: {rule}");
        assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
    }

    let trigger = parsed.abilities[0].rule.as_ref().expect("land-play rule");
    assert_eq!(trigger["event"]["kind"], "landPlayed");
    assert_eq!(trigger["event"]["excludeSource"], true);
    assert_eq!(trigger["effects"][0]["kind"], "sacrificePermanent");
    assert_eq!(trigger["effects"][0]["permanent"]["kind"], "self");

    let mana = parsed.abilities[1].rule.as_ref().expect("mana rule");
    assert_eq!(mana["kind"], "manaAbility");
    assert_eq!(mana["effects"][0]["kind"], "addMana");
    assert_eq!(mana["effects"][0]["mana"], "{C}{C}");
}
