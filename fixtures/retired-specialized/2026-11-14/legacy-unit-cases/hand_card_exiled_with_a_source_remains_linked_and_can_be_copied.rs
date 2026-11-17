#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn hand_card_exiled_with_a_source_remains_linked_and_can_be_copied() {
    let mut engine = test_engine(2);
    let source = test_instance(
        "imprint-source",
        test_definition("imprint-source", "Artifact"),
        "player-0",
    );
    let mut instant_definition = test_definition("imprinted-instant", "Instant");
    instant_definition.mana_cost = "{1}{U}".to_string();
    let instant = test_instance("imprinted-instant", instant_definition, "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[0].hand.push(instant);
    let stack_object = StackObject {
        id: "stack:imprint".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "handCardToExile".to_string(),
            TargetRef::Card {
                instance_id: "imprinted-instant".to_string(),
            },
        )]),
    };
    let exile = json!({
        "kind": "exileTargetCardWithSource",
        "card": { "kind": "chosenTarget", "id": "handCardToExile" },
        "source": { "kind": "self" },
        "fromZone": "hand",
    });
    let copy = json!({
        "kind": "createCardCopy",
        "card": { "kind": "cardExiledWithSource" },
        "fromZone": "exile",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "to": "exile",
        "bind": "copiedLinkedCard",
    });
    let mut bindings = BTreeMap::new();
    let mut decisions = BTreeMap::new();
    let mut provider = EmeritusDecisionProvider;

    engine
        .execute_effect(
            &exile,
            None,
            &stack_object,
            &mut bindings,
            &mut decisions,
            &mut provider,
        )
        .expect("the selected hand card is exiled and linked");
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .all(|card| card.instance_id != "imprinted-instant")
    );
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "imprinted-instant")
    );
    engine
        .execute_effect(
            &copy,
            None,
            &stack_object,
            &mut bindings,
            &mut decisions,
            &mut provider,
        )
        .expect("the linked exiled card can be copied");
    let RuntimeBinding::Objects(copy_ids) = &bindings["copiedLinkedCard"] else {
        panic!("the copied linked card should be bound");
    };
    assert_eq!(copy_ids.len(), 1);
    assert!(copy_ids[0].starts_with("copy:card:imprinted-instant:"));
}
