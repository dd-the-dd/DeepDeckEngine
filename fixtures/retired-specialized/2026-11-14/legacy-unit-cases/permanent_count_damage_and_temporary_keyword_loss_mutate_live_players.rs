#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn permanent_count_damage_and_temporary_keyword_loss_mutate_live_players() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(test_instance(
        "basic-land",
        test_definition("basic-land", "Basic Land - Island"),
        "player-0",
    ));
    let mut nonbasic = test_instance(
        "nonbasic-land",
        test_definition("nonbasic-land", "Land"),
        "player-1",
    );
    nonbasic.definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "hexproof" },
    })];
    engine.state.players[1].battlefield.push(nonbasic);
    let stack_object = StackObject {
        id: "stack:count-and-keyword".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "effect-source",
            test_definition("effect-source", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
            .execute_effects(
                &json!([
                    {
                        "kind": "dealDamageByPermanentCountEachPlayer",
                        "where": {
                            "kind": "and",
                            "operands": [
                                { "kind": "cardTypeContains", "value": "Land" },
                                { "kind": "not", "operand": { "kind": "typeLineContains", "value": "Basic" } },
                            ],
                        },
                        "factor": { "kind": "integer", "value": 2 },
                    },
                    {
                        "kind": "removeKeywordsFromPermanents",
                        "objects": {
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                            },
                            "where": null,
                        },
                        "keywords": ["hexproof"],
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ]),
                &stack_object,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut RandomAi::seeded(31),
            )
            .expect("generic count damage and keyword loss resolve");
    assert_eq!(engine.state.players[0].life, 20);
    assert_eq!(engine.state.players[1].life, 18);
    assert!(!engine.permanent_has_keyword(&engine.state.players[1].battlefield[0], "hexproof"));
}
