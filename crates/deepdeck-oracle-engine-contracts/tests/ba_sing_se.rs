use deepdeck_oracle_engine_contracts::oracle::{
    OracleCardParseRequest, RecognizerProvenance, parse_oracle_card,
};

const ORACLE: &str = "This land enters tapped unless you control a basic land.\n{T}: Add {G}.\n{2}{G}, {T}: Earthbend 2. Activate only as a sorcery. (Target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)";

#[test]
fn ba_sing_se_rules_are_native_and_earthbend_is_primitive_ir() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Ba Sing Se".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 3);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let earthbend = parsed.abilities[2].rule.as_ref().expect("Earthbend rule");
    let kinds = earthbend["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "becomeCreature",
            "grantKeyword",
            "putCounters",
            "installDelayedZoneChangeTrigger",
            "emitAbilitySignal",
        ]
    );
    let ir = earthbend.to_string();
    assert!(!ir.contains("\"kind\":\"earthbend\""));
    assert!(!ir.contains("resolveTriggeredInstruction"));
}
