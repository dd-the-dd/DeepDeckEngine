#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn optional_hand_discard_binds_the_draw_and_story_damage_amount() {
    let mut engine = test_engine(3);
    engine.state.players[0].hand = (0..3)
        .map(|index| {
            let id = format!("discarded-hand-{index}");
            test_instance(&id, test_definition(&id, "Creature"), "player-0")
        })
        .collect();
    engine.state.players[0].library = (0..6)
        .map(|index| {
            let id = format!("replacement-draw-{index}");
            test_instance(&id, test_definition(&id, "Basic Land - Plains"), "player-0")
        })
        .collect();
    engine.state.players[0].graveyard.clear();
    engine.state.rule_modifiers.push(json!({
        "kind": "enduringStory",
        "playerId": "player-0",
    }));
    let stack_object = StackObject {
        id: "stack:optional-hand-discard".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "optional-hand-discard",
            test_definition("optional-hand-discard", "Creature - Dwarf"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };

    engine
        .execute_effects(
            &json!([{
                "kind": "optionalAction",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "action": {
                    "kind": "discardHand",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "bindCountAs": "discardedHandCount",
                },
                "onPerformed": [
                    {
                        "kind": "drawCards",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "count": { "kind": "boundValue", "id": "discardedHandCount" },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": {
                            "kind": "hasEnduringStory",
                            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        },
                        "then": [{
                            "kind": "dealDamageToEachOpponent",
                            "amount": { "kind": "boundValue", "id": "discardedHandCount" },
                        }],
                        "else": [],
                    },
                ],
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the optional discard, linked draw, and damage resolve");

    assert_eq!(engine.state.players[0].hand.len(), 3);
    assert_eq!(engine.state.players[0].graveyard.len(), 3);
    assert_eq!(engine.state.players[0].library.len(), 3);
    assert_eq!(engine.state.players[1].life, 17);
    assert_eq!(engine.state.players[2].life, 17);
}
