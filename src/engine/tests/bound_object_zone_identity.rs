use super::*;

fn stack_object() -> StackObject {
    StackObject {
        id: "stack:bound-object-zone-identity".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "binding-source",
            test_definition("binding-source", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn single_bound_object() -> Value {
    json!({
        "kind": "singleBoundObject",
        "binding": "trackedObject",
    })
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn single_bound_object_tracks_a_returned_permanent_in_its_current_zone() {
    let mut engine = test_engine(2);
    let tracked = test_instance(
        "tracked-permanent",
        test_definition("tracked-permanent", "Creature"),
        "player-0",
    );
    engine.state.players[0].exile.push(tracked);

    let stack_object = stack_object();
    let bindings = BTreeMap::from([(
        "trackedObject".to_string(),
        RuntimeBinding::Objects(vec!["tracked-permanent".to_string()]),
    )]);
    let reference = single_bound_object();
    let controller = json!({ "kind": "controllerOf", "object": reference.clone() });

    assert_eq!(
        engine.resolve_target(&reference, &stack_object, &bindings),
        Some(TargetRef::Card {
            instance_id: "tracked-permanent".to_string(),
        })
    );
    assert_eq!(
        engine.resolve_player(&controller, &stack_object, &bindings),
        Some("player-0".to_string()),
        "outside the battlefield, controllerOf follows the card owner"
    );

    let mut returned = engine.state.players[0]
        .exile
        .pop()
        .expect("the tracked card starts in exile");
    returned.controller = "player-1".to_string();
    engine.state.players[1].battlefield.push(returned);

    assert_eq!(
        engine.resolve_target(&reference, &stack_object, &bindings),
        Some(TargetRef::Permanent {
            instance_id: "tracked-permanent".to_string(),
        })
    );
    assert_eq!(
        engine.resolve_player(&controller, &stack_object, &bindings),
        Some("player-1".to_string()),
        "after the return, controllerOf must query the battlefield permanent"
    );
    assert_eq!(
        engine.resolve_player(
            &json!({ "kind": "ownerOf", "object": reference }),
            &stack_object,
            &bindings,
        ),
        Some("player-0".to_string()),
        "changing control must not change the returned permanent's owner"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn single_bound_object_remains_a_card_while_outside_the_battlefield() {
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.push(test_instance(
        "tracked-card",
        test_definition("tracked-card", "Creature"),
        "player-0",
    ));
    let bindings = BTreeMap::from([(
        "trackedObject".to_string(),
        RuntimeBinding::Objects(vec!["tracked-card".to_string()]),
    )]);

    assert_eq!(
        engine.resolve_target(&single_bound_object(), &stack_object(), &bindings),
        Some(TargetRef::Card {
            instance_id: "tracked-card".to_string(),
        })
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn exile_binding_contains_only_permanents_that_actually_moved_to_exile() {
    let effect = json!({
        "kind": "exilePermanent",
        "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
        "bind": "exiledPermanents",
    });

    let mut moved_engine = test_engine(2);
    moved_engine.state.players[0]
        .battlefield
        .push(test_instance(
            "present-permanent",
            test_definition("present-permanent", "Creature"),
            "player-0",
        ));
    let mut moved_stack = stack_object();
    moved_stack.targets.insert(
        "targetPermanent".to_string(),
        TargetRef::Permanent {
            instance_id: "present-permanent".to_string(),
        },
    );
    let mut moved_bindings = BTreeMap::new();
    moved_engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    moved_engine
        .execute_effects(
            &json!([effect.clone()]),
            &moved_stack,
            &mut moved_bindings,
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the present permanent is exiled");
    let Some(RuntimeBinding::Objects(moved_ids)) = moved_bindings.get("exiledPermanents") else {
        panic!("exilePermanent must populate its object binding");
    };
    assert_eq!(moved_ids, &["present-permanent".to_string()]);

    let mut missing_engine = test_engine(2);
    missing_engine.state.players[0].exile.push(test_instance(
        "already-absent-permanent",
        test_definition("already-absent-permanent", "Creature"),
        "player-0",
    ));
    let mut missing_stack = stack_object();
    missing_stack.targets.insert(
        "targetPermanent".to_string(),
        TargetRef::Permanent {
            instance_id: "already-absent-permanent".to_string(),
        },
    );
    let mut missing_bindings = BTreeMap::new();
    missing_engine
        .execute_effects(
            &json!([effect]),
            &missing_stack,
            &mut missing_bindings,
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("an absent target is a no-op");
    let Some(RuntimeBinding::Objects(missing_ids)) = missing_bindings.get("exiledPermanents")
    else {
        panic!("an optional failed exile must still populate an empty binding");
    };
    assert!(
        missing_ids.is_empty(),
        "a card that was already outside the battlefield was not exiled by this effect"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn move_cards_source_zone_constraint_does_not_pull_a_bound_card_from_another_zone() {
    let effect = json!({
        "kind": "moveCards",
        "cards": { "kind": "boundObjects", "binding": "trackedObject" },
        "from": { "kind": "exile", "player": { "kind": "abilityController" } },
        "to": {
            "kind": "battlefield",
            "player": { "kind": "abilityController" },
            "tapped": false,
        },
        "bind": "returnedPermanents",
    });
    let rule = json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [effect.clone()],
    });
    assert!(
        rule_is_executable(&rule),
        "moveCards should accept a generic source-zone constraint"
    );

    let bindings = || {
        BTreeMap::from([(
            "trackedObject".to_string(),
            RuntimeBinding::Objects(vec!["tracked-card".to_string()]),
        )])
    };
    let mut wrong_zone_engine = test_engine(2);
    wrong_zone_engine.state.players[0]
        .graveyard
        .push(test_instance(
            "tracked-card",
            test_definition("tracked-card", "Creature"),
            "player-0",
        ));
    let mut wrong_zone_bindings = bindings();
    wrong_zone_engine
        .execute_effects(
            &json!([effect.clone()]),
            &stack_object(),
            &mut wrong_zone_bindings,
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("a source-zone mismatch is a no-op");
    assert!(wrong_zone_engine.state.players[0].battlefield.is_empty());
    assert_eq!(wrong_zone_engine.state.players[0].graveyard.len(), 1);
    let Some(RuntimeBinding::Objects(returned_ids)) = wrong_zone_bindings.get("returnedPermanents")
    else {
        panic!("moveCards must populate its output binding even when nothing moved");
    };
    assert!(returned_ids.is_empty());

    let mut exile_engine = test_engine(2);
    exile_engine.state.players[0].exile.push(test_instance(
        "tracked-card",
        test_definition("tracked-card", "Creature"),
        "player-0",
    ));
    let mut exile_bindings = bindings();
    exile_engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    exile_engine
        .execute_effects(
            &json!([effect]),
            &stack_object(),
            &mut exile_bindings,
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the matching exiled card returns");
    assert!(exile_engine.state.players[0].exile.is_empty());
    assert_eq!(
        exile_engine.state.players[0].battlefield[0].instance_id,
        "tracked-card"
    );
    let Some(RuntimeBinding::Objects(returned_ids)) = exile_bindings.get("returnedPermanents")
    else {
        panic!("moveCards must bind the object it returned");
    };
    assert_eq!(returned_ids, &["tracked-card".to_string()]);
}
