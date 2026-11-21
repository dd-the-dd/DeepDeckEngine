#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn linked_exile_graveyard_bottom_and_evoke_mutate_live_state() {
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.extend([
        test_instance(
            "grave-card-a",
            test_definition("grave-card-a", "Creature"),
            "player-0",
        ),
        test_instance(
            "grave-card-b",
            test_definition("grave-card-b", "Instant"),
            "player-0",
        ),
    ]);
    let initial_library_len = engine.state.players[0].library.len();
    let stack_object = StackObject {
        id: "stack:graveyard-bottom".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "graveyard-source",
            test_definition("graveyard-source", "Creature"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetPlayer".to_string(),
            TargetRef::Player {
                player_id: "player-0".to_string(),
            },
        )]),
    };
    engine
        .execute_effects(
            &json!([{
                "kind": "moveGraveyardToLibraryBottomRandom",
                "player": { "kind": "chosenTarget", "id": "targetPlayer" },
            }]),
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut RandomAi::seeded(47),
        )
        .expect("a targeted graveyard moves to the library bottom");
    assert!(engine.state.players[0].graveyard.is_empty());
    assert_eq!(
        engine.state.players[0].library.len(),
        initial_library_len + 2
    );

    let linked_count = json!({
        "kind": "countCardsExiledWithSource",
        "object": { "kind": "self" },
    });
    let mut characteristic_definition = test_definition("linked-source", "Creature");
    characteristic_definition.power = Some("*".to_string());
    characteristic_definition.toughness = Some("*".to_string());
    characteristic_definition.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "setBasePowerToughness",
            "objects": { "kind": "self" },
            "power": linked_count.clone(),
            "toughness": linked_count,
        }],
    })];
    let linked_source = test_instance("linked-source", characteristic_definition, "player-0");
    engine.state.players[0]
        .battlefield
        .push(linked_source.clone());
    for card_id in ["linked-a", "linked-b"] {
        engine.state.rule_modifiers.push(json!({
            "kind": "exiledWithSource",
            "sourceCardInstanceId": "linked-source",
            "cardInstanceId": card_id,
        }));
    }
    assert_eq!(engine.current_power(&linked_source), 2);
    assert_eq!(engine.current_toughness(&linked_source), 2);

    let mut evoke = test_instance(
        "evoke-source",
        test_definition("evoke-source", "Creature"),
        "player-0",
    );
    evoke
        .flags
        .insert("castWithAlternativeCost:evoke".to_string(), true);
    engine.state.players[0].battlefield.push(evoke.clone());
    engine.enqueue_enter_battlefield_triggers(&evoke);
    assert!(engine.state.stack.iter().any(|object| {
        object
            .ability_rule
            .as_ref()
            .is_some_and(|rule| rule["effects"][0]["kind"] == "sacrificePermanent")
    }));
    engine
        .resolve_top_stack(&mut RandomAi::seeded(53))
        .expect("the evoke sacrifice trigger resolves");
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "evoke-source")
    );
}
