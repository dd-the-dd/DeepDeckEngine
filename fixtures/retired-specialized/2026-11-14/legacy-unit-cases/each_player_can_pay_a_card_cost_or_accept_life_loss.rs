#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn each_player_can_pay_a_card_cost_or_accept_life_loss() {
    let mut engine = test_engine(2);
    let stack_object = StackObject {
        id: "stack:each-player-cost".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "each-player-cost-source",
            test_definition("each-player-cost-source", "Dungeon"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("dungeonRoom".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let initial_hand_sizes = engine
        .state
        .players
        .iter()
        .map(|player| player.hand.len())
        .collect::<Vec<_>>();
    engine
        .execute_effects(
            &json!([{
                "kind": "eachPlayerPaysCostOrLosesLife",
                "cost": { "kind": "discardCard", "where": null },
                "amount": { "kind": "integer", "value": 2 },
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("each player may choose a matching card to discard");
    assert!(
        engine
            .state
            .players
            .iter()
            .enumerate()
            .all(|(index, player)| {
                player.life == 20 && player.hand.len() + 1 == initial_hand_sizes[index]
            })
    );

    for player_index in 0..2 {
        engine.state.players[player_index]
            .battlefield
            .push(test_instance(
                &format!("cost-creature-{player_index}"),
                test_definition(&format!("cost-creature-{player_index}"), "Creature"),
                &format!("player-{player_index}"),
            ));
    }
    engine
        .execute_effects(
            &json!([{
                "kind": "eachPlayerPaysCostOrLosesLife",
                "cost": {
                    "kind": "sacrificePermanent",
                    "where": {
                        "kind": "or",
                        "operands": [
                            { "kind": "cardTypeContains", "value": "Creature" },
                            { "kind": "cardTypeContains", "value": "Artifact" },
                            { "kind": "cardTypeContains", "value": "Land" },
                        ],
                    },
                },
                "amount": { "kind": "integer", "value": 2 },
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("each player may sacrifice a permanent matching disjunctive criteria");
    assert!(
        engine
            .state
            .players
            .iter()
            .all(|player| player.life == 20 && player.battlefield.is_empty())
    );

    for player in &mut engine.state.players {
        player.hand.clear();
    }
    engine
        .execute_effects(
            &json!([{
                "kind": "eachPlayerPaysCostOrLosesLife",
                "cost": { "kind": "discardCard", "where": null },
                "amount": { "kind": "integer", "value": 2 },
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("a player without a payable card takes the consequence");
    assert!(engine.state.players.iter().all(|player| player.life == 18));
}
