use super::*;

const ORACLE: &str = "As an additional cost to cast this spell, you may blight 1. (You may put a -1/-1 counter on a creature you control.)\nDestroy target creature with mana value 2 or less. If this spell's additional cost was paid, you gain 2 life.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn requiting_hex_paid_blight_crosses_rules_and_executes_all_primitives() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Requiting Hex".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == crate::oracle::RecognizerProvenance::Native)
    );
    let rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("each ability emits primitive IR"))
        .collect::<Vec<_>>();
    assert_eq!(rules.len(), 2);
    assert!(rules.iter().all(|rule| rule_is_executable(rule)));

    let mut spell_definition = test_definition("requiting-hex", "Instant");
    spell_definition.mana_cost = "{B}".to_string();
    spell_definition.rules = rules;
    let mut blight_creature_definition = test_definition("blight-creature", "Creature");
    blight_creature_definition.power = Some("3".to_string());
    blight_creature_definition.toughness = Some("3".to_string());
    let mut target_definition = test_definition("qualified-target", "Creature");
    target_definition.mana_cost = "{2}".to_string();
    target_definition.power = Some("2".to_string());
    target_definition.toughness = Some("2".to_string());

    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = vec![FloatingMana {
        symbol: "B".to_string(),
        spend_restriction: None,
    }];
    engine.state.players[0].hand = vec![test_instance(
        "requiting-hex-instance",
        spell_definition,
        "player-0",
    )];
    engine.state.players[0].battlefield = vec![test_instance(
        "blight-creature-instance",
        blight_creature_definition,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![test_instance(
        "qualified-target-instance",
        target_definition,
        "player-1",
    )];

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("requiting-hex-instance")
                && action.decisions["additionalCostMode"] == json!(["pay"])
                && action.targets.get("blightCost1")
                    == Some(&TargetRef::Permanent {
                        instance_id: "blight-creature-instance".to_string(),
                    })
                && action.targets.get("targetPermanent")
                    == Some(&TargetRef::Permanent {
                        instance_id: "qualified-target-instance".to_string(),
                    })
        })
        .expect("a real paid-Blight cast declaration targets both creatures");

    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(13))
        .expect("the paid-Blight declaration casts the spell");
    assert_eq!(engine.state.players[0].battlefield[0].counters["-1/-1"], 1);
    assert_eq!(engine.state.stack.len(), 1);

    engine
        .resolve_top_stack(&mut RandomAi::seeded(14))
        .expect("Requiting Hex resolves through primitive runtime effects");
    assert_eq!(engine.state.players[0].life, 22);
    assert!(engine.state.players[1].battlefield.is_empty());
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "qualified-target-instance")
    );
}
