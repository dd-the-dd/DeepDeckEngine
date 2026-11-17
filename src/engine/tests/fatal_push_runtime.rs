use super::*;

const ORACLE: &str = "Destroy target creature if it has mana value 2 or less.\nRevolt — Destroy that creature if it has mana value 4 or less instead if a permanent left the battlefield under your control this turn.";

fn fatal_push_rules() -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Fatal Push".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B}".to_string()),
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
            .all(|ability| ability.recognizer == crate::oracle::RecognizerProvenance::Native)
    );

    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("each ability emits primitive IR"))
        .inspect(|rule| assert!(rule_is_executable(rule)))
        .collect()
}

fn resolve_fatal_push(target_mana_value: u32, revolt: bool) -> GameEngine {
    let mut spell_definition = test_definition("fatal-push", "Instant");
    spell_definition.mana_cost = "{B}".to_string();
    spell_definition.rules = fatal_push_rules();

    let mut target_definition = test_definition("fatal-push-target", "Creature");
    target_definition.mana_cost = format!("{{{target_mana_value}}}");
    target_definition.power = Some("3".to_string());
    target_definition.toughness = Some("3".to_string());

    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = vec![FloatingMana {
        symbol: "B".to_string(),
        spend_restriction: None,
    }];
    engine.state.players[0].hand = vec![test_instance(
        "fatal-push-instance",
        spell_definition,
        "player-0",
    )];
    engine.state.players[1].battlefield = vec![test_instance(
        "fatal-push-target-instance",
        target_definition,
        "player-1",
    )];
    if revolt {
        engine.state.events.push(GameEvent {
            sequence: engine.state.events.len() + 1,
            turn_number: engine.state.turn_number,
            step: GameStep::PrecombatMain,
            kind: "permanentLeftBattlefield".to_string(),
            player_id: Some("player-0".to_string()),
            card_instance_id: Some("departed-permanent".to_string()),
            detail: json!({
                "fromZone": "battlefield",
                "destination": "graveyard",
                "typeLine": "Creature",
            }),
        });
    }

    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("fatal-push-instance")
                && action.targets.get("targetPermanent")
                    == Some(&TargetRef::Permanent {
                        instance_id: "fatal-push-target-instance".to_string(),
                    })
        })
        .expect("Fatal Push can declare any target creature before its threshold is checked");

    engine
        .apply_priority_action(&action, &mut RandomAi::seeded(83))
        .expect("Fatal Push is cast through the primitive declaration");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(84))
        .expect("Fatal Push resolves through conditional primitive effects");
    engine
}

fn target_was_destroyed(engine: &GameEngine) -> bool {
    engine.state.players[1]
        .graveyard
        .iter()
        .any(|card| card.instance_id == "fatal-push-target-instance")
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn fatal_push_runtime_discriminates_base_and_revolt_thresholds() {
    assert!(
        target_was_destroyed(&resolve_fatal_push(2, false)),
        "the base effect destroys a creature with mana value 2"
    );
    assert!(
        !target_was_destroyed(&resolve_fatal_push(3, false)),
        "mana value 3 survives when revolt is not enabled"
    );
    assert!(
        target_was_destroyed(&resolve_fatal_push(3, true)),
        "the event-history condition enables the amended mana-value-4 threshold"
    );
    assert!(
        !target_was_destroyed(&resolve_fatal_push(5, true)),
        "revolt does not destroy a creature above the amended threshold"
    );
}
