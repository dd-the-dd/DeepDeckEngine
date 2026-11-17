#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn per_opponent_linked_exile_targets_distinct_controllers_and_returns_every_card() {
    let mut engine = test_engine(4);
    let effect = json!({
        "kind": "exilePermanentWithSource",
        "permanent": { "kind": "chosenTargets", "id": "exileTargets" },
        "candidates": {
            "kind": "permanents",
            "controller": {
                "kind": "opponentsOf",
                "player": { "kind": "abilityController" },
            },
            "where": { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Land" } },
        },
        "minimum": 0,
        "maximum": {
            "kind": "countOpponents",
            "player": { "kind": "abilityController" },
        },
        "returnWhenSourceLeaves": true,
    });
    let rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "kind": "chooseTargets",
                "id": "exileTargets",
                "minimum": 0,
                "maximum": {
                    "kind": "countOpponents",
                    "player": { "kind": "abilityController" },
                },
                "candidates": effect["candidates"].clone(),
                "selectionConstraint": { "kind": "distinctPermanentControllers" },
            }],
        },
        "effects": [effect.clone()],
    });
    let mut source = test_instance(
        "mountain-king",
        test_definition("mountain-king", "Enchantment"),
        "player-0",
    );
    source.definition.rules.push(rule.clone());
    engine.state.players[0].battlefield.push(source.clone());
    for player_index in 1..4 {
        for permanent_index in 0..2 {
            let instance_id = format!("victim-{player_index}-{permanent_index}");
            engine.state.players[player_index]
                .battlefield
                .push(test_instance(
                    &instance_id,
                    test_definition(&instance_id, "Creature - Test"),
                    &format!("player-{player_index}"),
                ));
        }
    }

    let declarations = engine
        .declarations_for_activated_rule(&rule, "player-0", "mountain-king")
        .expect("per-opponent target declarations are executable");
    assert!(
        declarations
            .iter()
            .any(|declaration| declaration.targets.len() == 3)
    );
    assert!(declarations.iter().all(|declaration| {
        let controllers =
            declaration
                .targets
                .values()
                .filter_map(|target| match target {
                    TargetRef::Permanent { instance_id } => engine
                        .permanent_position(instance_id)
                        .map(|(player_index, card_index)| {
                            engine.state.players[player_index].battlefield[card_index]
                                .controller
                                .clone()
                        }),
                    _ => None,
                })
                .collect::<BTreeSet<_>>();
        controllers.len() == declaration.targets.len()
    }));

    let stack_object = StackObject {
        id: "stack:per-opponent-linked-exile".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule),
        decisions: BTreeMap::new(),
        targets: (1..4)
            .map(|player_index| {
                (
                    format!("exileTargets:{}", player_index - 1),
                    TargetRef::Permanent {
                        instance_id: format!("victim-{player_index}-0"),
                    },
                )
            })
            .collect(),
    };
    let mut provider = EmeritusDecisionProvider;
    engine
        .execute_effect(
            &effect,
            None,
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut provider,
        )
        .expect("all declared per-opponent targets are exiled");
    for player_index in 1..4 {
        assert!(
            engine
                .permanent_position(&format!("victim-{player_index}-0"))
                .is_none()
        );
    }

    engine
        .move_permanent_to_graveyard("mountain-king", "destroyed")
        .expect("source leaves");
    engine
        .resolve_top_stack(&mut provider)
        .expect("linked cards return");
    for player_index in 1..4 {
        assert!(
            engine
                .permanent_position(&format!("victim-{player_index}-0"))
                .is_some()
        );
    }
}
