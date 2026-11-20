use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};

const ORACLE: &str = "{T}: Add {C}.\n{4}: This land becomes a 3/3 creature with vigilance and all creature types. It's still a land.";

#[test]
fn soulstone_sanctuary_is_native_primitive_and_executable() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Soulstone Sanctuary".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability.rule.as_ref().is_some_and(rule_is_executable)
    }));
    let animation = parsed.abilities[1].rule.as_ref().unwrap();
    assert_eq!(
        animation["effects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|effect| effect["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "addCardType",
            "modifySubtypeSet",
            "grantKeyword",
            "setBasePowerToughness"
        ]
    );
}
