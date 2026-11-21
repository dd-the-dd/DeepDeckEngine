#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn linked_exile_returns_the_card_when_the_source_leaves() {
    let mut engine = test_engine(2);
    let prison = test_instance(
        "prison",
        test_definition("prison", "Enchantment"),
        "player-0",
    );
    let victim = test_instance(
        "victim",
        test_definition("victim", "Creature - Bear"),
        "player-1",
    );
    engine.state.players[0].battlefield.push(prison.clone());
    engine.state.players[1].battlefield.push(victim);
    let stack_object = StackObject {
        id: "stack:linked-exile".to_string(),
        controller: "player-0".to_string(),
        card: prison,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let effect = json!({
        "kind": "exilePermanentWithSource",
        "permanent": { "kind": "chosenTarget", "id": "exileTarget" },
        "candidates": {
            "kind": "permanents",
            "controller": {
                "kind": "opponentsOf",
                "player": { "kind": "abilityController" },
            },
            "where": { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Land" } },
        },
        "minimum": 1,
        "maximum": 1,
        "returnWhenSourceLeaves": true,
    });
    let mut bindings = BTreeMap::new();
    let mut decisions = BTreeMap::new();
    let mut provider = EmeritusDecisionProvider;

    engine
        .execute_effect(
            &effect,
            None,
            &stack_object,
            &mut bindings,
            &mut decisions,
            &mut provider,
        )
        .expect("linked exile resolves");
    assert!(engine.permanent_position("victim").is_none());
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "victim")
    );

    engine
        .move_permanent_to_graveyard("prison", "destroyed")
        .expect("source leaves");
    engine
        .resolve_top_stack(&mut provider)
        .expect("linked return trigger resolves");

    assert!(engine.permanent_position("victim").is_some());
    assert!(
        !engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "victim")
    );
}
