use deepdeck_oracle_engine_contracts::oracle::{
    OracleCardParseRequest, RecognizerProvenance, parse_oracle_card,
};

const ORACLE: &str = "Mobilize 2 (Whenever this creature attacks, create two tapped and attacking 1/1 red Warrior creature tokens. Sacrifice them at the beginning of the next end step.)\nYour opponents can't cast spells during your turn.";

#[test]
fn voice_rules_are_native_and_mobilize_has_only_primitive_effects() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Voice of Victory".to_string(),
        type_line: "Creature - Human Bard".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let mobilize = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("native Mobilize rule");
    assert_eq!(mobilize["kind"], "triggeredAbility");
    assert_eq!(mobilize["effects"][0]["kind"], "createTokens");
    assert_eq!(mobilize["effects"][1]["kind"], "installDelayedStepTrigger");
    assert_eq!(
        mobilize["effects"][1]["effects"][0]["kind"],
        "sacrificePermanents"
    );
    assert_eq!(
        mobilize["effects"][1]["effects"][0]["player"]["kind"],
        "controllerOf"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["modifiers"][0]["kind"],
        "prohibitCast"
    );

    let all_ir = parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .map(ToString::to_string)
        .collect::<String>();
    for forbidden in [
        "keywordAbility",
        "sacrificeAtNextEndStep",
        "resolveTriggeredInstruction",
        "emitAbilitySignal",
    ] {
        assert!(!all_ir.contains(forbidden), "forbidden IR: {forbidden}");
    }
}
