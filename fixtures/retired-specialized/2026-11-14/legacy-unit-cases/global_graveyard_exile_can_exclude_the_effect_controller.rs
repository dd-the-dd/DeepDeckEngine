#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn global_graveyard_exile_can_exclude_the_effect_controller() {
    let mut engine = test_engine(3);
    for player_index in 0..3 {
        let player_id = format!("player-{player_index}");
        engine.state.players[player_index].graveyard = vec![test_instance(
            &format!("grave-card-{player_index}"),
            test_definition(&format!("grave-card-{player_index}"), "Sorcery"),
            &player_id,
        )];
    }
    let stack_object = StackObject {
        id: "stack:opponent-graveyard-exile".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "graveyard-exile-source",
            test_definition("graveyard-exile-source", "Artifact"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };

    engine
        .execute_effects(
            &json!([{
                "kind": "exileAllGraveyards",
                "exceptPlayer": { "kind": "controllerOf", "object": { "kind": "self" } },
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut RandomAi::seeded(17),
        )
        .expect("opponent graveyards are exiled");

    assert_eq!(engine.state.players[0].graveyard.len(), 1);
    assert!(engine.state.players[0].exile.is_empty());
    for player_index in 1..3 {
        assert!(engine.state.players[player_index].graveyard.is_empty());
        assert_eq!(engine.state.players[player_index].exile.len(), 1);
    }
}
