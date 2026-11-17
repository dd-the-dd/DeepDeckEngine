use super::*;

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn primitive_barrowgoyf_characteristic_and_bound_mill_execute_together() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/barrowgoyf.json"
    ))
    .expect("Barrowgoyf ground-truth fixture is valid JSON");
    let mut source_definition = test_definition("barrowgoyf", "Creature - Lhurgoyf");
    source_definition.power = Some("*".to_string());
    source_definition.toughness = Some("1+*".to_string());
    source_definition.rules = fixture["abilities"]
        .as_array()
        .expect("fixture abilities")
        .iter()
        .map(|ability| ability["expectedRule"].clone())
        .collect();

    let mut engine = test_engine(2);
    let source = test_instance("barrowgoyf", source_definition, "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].graveyard.push(test_instance(
        "artifact-creature",
        test_definition("artifact-creature", "Artifact Creature"),
        "player-0",
    ));
    engine.state.players[1].graveyard.push(test_instance(
        "instant",
        test_definition("instant", "Instant"),
        "player-1",
    ));

    assert_eq!(engine.current_power(&source), 3);
    assert_eq!(engine.current_toughness(&source), 4);

    engine.state.players[0].library = vec![
        test_instance(
            "milled-sorcery",
            test_definition("milled-sorcery", "Sorcery"),
            "player-0",
        ),
        test_instance(
            "milled-creature",
            test_definition("milled-creature", "Creature"),
            "player-0",
        ),
        test_instance(
            "milled-land",
            test_definition("milled-land", "Land"),
            "player-0",
        ),
    ];
    let hand_before = engine.state.players[0].hand.len();
    engine.enqueue_combat_damage_to_player_triggers("barrowgoyf", "player-1", 3);
    assert_eq!(engine.state.stack.len(), 1);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the optional mill and recovery trigger resolves");

    assert!(engine.state.players[0].library.is_empty());
    assert_eq!(engine.state.players[0].hand.len(), hand_before + 1);
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "milled-creature")
    );
}
