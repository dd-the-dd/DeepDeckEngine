#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn linked_hand_or_permanent_exile_returns_the_selected_permanent() {
    let mut engine = test_engine(2);
    let source = test_instance(
        "linked-exile-source",
        test_definition("linked-exile-source", "Enchantment"),
        "player-0",
    );
    engine.state.players[0].battlefield.extend([
        source.clone(),
        test_instance(
            "wrong-controller-creature",
            test_definition("wrong-controller-creature", "Creature"),
            "player-0",
        ),
    ]);
    engine.state.players[1].battlefield.push(test_instance(
        "chosen-creature",
        test_definition("chosen-creature", "Creature"),
        "player-1",
    ));
    engine.state.players[1].hand.push(test_instance(
        "revealed-nonland",
        test_definition("revealed-nonland", "Instant"),
        "player-1",
    ));
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "entersBattlefield", "object": { "kind": "self" } },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [
                {
                    "kind": "chooseTargets",
                    "id": "targetOpponent",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "players",
                        "where": {
                            "kind": "isOpponentOf",
                            "player": { "kind": "abilityController" },
                        },
                    },
                },
                {
                    "kind": "chooseTargets",
                    "id": "targetCreature",
                    "minimum": 0,
                    "maximum": 1,
                    "candidates": {
                        "kind": "permanents",
                        "where": { "kind": "cardTypeContains", "value": "Creature" },
                    },
                    "selectionConstraint": {
                        "kind": "targetControllerMatchesTargetPlayer",
                        "targetId": "targetOpponent",
                    },
                },
            ],
        },
        "effects": [{
            "kind": "chooseExileFromHandOrPermanentUntilSourceLeaves",
            "player": { "kind": "chosenTarget", "id": "targetOpponent" },
            "permanent": { "kind": "chosenTarget", "id": "targetCreature" },
            "handWhere": {
                "kind": "not",
                "operand": { "kind": "cardTypeContains", "value": "Land" },
            },
            "optional": true,
        }],
    });
    let declarations = engine
        .declarations_for_triggered_rule(&rule, "player-0", &source)
        .expect("linked player-controller declarations are supported");
    assert_eq!(declarations.len(), 2);
    assert!(declarations.iter().all(|declaration| {
        declaration
            .targets
            .get("targetCreature")
            .is_none_or(|target| {
                target
                    == &TargetRef::Permanent {
                        instance_id: "chosen-creature".to_string(),
                    }
            })
    }));

    let stack_object = StackObject {
        id: "stack:linked-hand-or-permanent".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([
            (
                "targetOpponent".to_string(),
                TargetRef::Player {
                    player_id: "player-1".to_string(),
                },
            ),
            (
                "targetCreature".to_string(),
                TargetRef::Permanent {
                    instance_id: "chosen-creature".to_string(),
                },
            ),
        ]),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the selected permanent is exiled");
    assert!(engine.permanent_position("chosen-creature").is_none());

    engine
        .move_permanent_to_graveyard("linked-exile-source", "destroyed")
        .expect("the source leaves the battlefield");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the linked return trigger resolves");
    assert!(engine.permanent_position("chosen-creature").is_some());
}
