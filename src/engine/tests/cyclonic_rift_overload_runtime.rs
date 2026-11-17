use super::*;

const ORACLE: &str = "Return target nonland permanent you don't control to its owner's hand.\nOverload {6}{U} (You may cast this spell for its overload cost. If you do, change \"target\" in its text to \"each.\")";

fn cyclonic_rift_definition() -> CardDefinition {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Cyclonic Rift".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    let mut definition = test_definition("cyclonic-rift", "Instant");
    definition.mana_cost = "{1}{U}".to_string();
    definition.rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("native ability emits a rule"))
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    definition
}

fn mana(symbols: &[&str]) -> Vec<FloatingMana> {
    symbols
        .iter()
        .map(|symbol| FloatingMana {
            symbol: (*symbol).to_string(),
            spend_restriction: None,
        })
        .collect()
}

fn setup_engine(mana_pool: Vec<FloatingMana>) -> GameEngine {
    let mut engine = test_engine(3);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].mana_pool = mana_pool;
    engine.state.players[0].hand = vec![test_instance(
        "cyclonic-rift",
        cyclonic_rift_definition(),
        "player-0",
    )];
    engine.state.players[0].battlefield = vec![
        test_instance(
            "own-artifact",
            test_definition("own-artifact", "Artifact"),
            "player-0",
        ),
        test_instance("own-land", test_definition("own-land", "Land"), "player-0"),
    ];
    engine.state.players[1].battlefield = vec![
        test_instance(
            "opponent-creature",
            test_definition("opponent-creature", "Creature"),
            "player-1",
        ),
        test_instance(
            "opponent-land",
            test_definition("opponent-land", "Land"),
            "player-1",
        ),
    ];
    let mut protected_definition = test_definition("other-player-enchantment", "Enchantment");
    protected_definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "hexproof" },
    })];
    engine.state.players[2].battlefield = vec![test_instance(
        "other-player-enchantment",
        protected_definition,
        "player-2",
    )];
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn normal_cast_requires_one_nonland_permanent_not_controlled_by_the_caster() {
    let mut engine = setup_engine(mana(&["C", "U"]));
    let casts = engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("cyclonic-rift")
        })
        .collect::<Vec<_>>();

    assert_eq!(
        casts.len(),
        1,
        "the normal cast cannot target the other player's hexproof permanent"
    );
    assert!(casts.iter().all(|action| {
        action.decisions.get("useAlternativeCost") == Some(&Value::Bool(false))
            && action.targets.len() == 1
    }));
    let cast = casts
        .iter()
        .find(|action| {
            action.targets.get("targetPermanent")
                == Some(&TargetRef::Permanent {
                    instance_id: "opponent-creature".to_string(),
                })
        })
        .expect("normal cast can target the opponent creature")
        .clone();

    engine
        .apply_priority_action(&cast, &mut RandomAi::seeded(12401))
        .expect("cast Cyclonic Rift normally");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(12402))
        .expect("resolve the chosen-target return");

    assert!(
        engine.state.players[1]
            .hand
            .iter()
            .any(|card| card.instance_id == "opponent-creature")
    );
    assert!(
        engine.state.players[2]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "other-player-enchantment")
    );
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "own-artifact")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn overload_has_no_targets_and_returns_each_other_player_nonland() {
    let mut engine = setup_engine(mana(&["C", "C", "C", "C", "C", "C", "U"]));
    let overload = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("cyclonic-rift")
                && action.decisions.get("useAlternativeCost") == Some(&Value::Bool(true))
                && action.decisions.get("alternativeCostMode")
                    == Some(&Value::String("overload".to_string()))
        })
        .expect("the overload casting method is legal");
    assert!(
        overload.targets.is_empty(),
        "an overloaded spell has no targets"
    );

    engine
        .apply_priority_action(&overload, &mut RandomAi::seeded(12403))
        .expect("cast Cyclonic Rift for its overload cost");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(12404))
        .expect("resolve the each-permanent return");

    assert!(
        engine.state.players[1]
            .hand
            .iter()
            .any(|card| card.instance_id == "opponent-creature")
    );
    assert!(
        engine.state.players[2]
            .hand
            .iter()
            .any(|card| card.instance_id == "other-player-enchantment")
    );
    for (player_index, permanent_id) in [(0, "own-artifact"), (0, "own-land"), (1, "opponent-land")]
    {
        assert!(
            engine.state.players[player_index]
                .battlefield
                .iter()
                .any(|card| card.instance_id == permanent_id)
        );
    }
}
