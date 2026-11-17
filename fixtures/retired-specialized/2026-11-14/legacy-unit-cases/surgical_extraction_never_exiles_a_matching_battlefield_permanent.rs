#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn surgical_extraction_never_exiles_a_matching_battlefield_permanent() {
    let mut engine = test_engine(2);
    let mut extracted_definition = test_definition("extracted-card", "Creature - Zombie Imp");
    extracted_definition.name = "Putrid Imp".to_string();
    let target = test_instance("putrid-graveyard", extracted_definition.clone(), "player-1");
    engine.state.players[1].graveyard.push(target.clone());
    engine.state.players[1].hand.push(test_instance(
        "putrid-hand",
        extracted_definition.clone(),
        "player-1",
    ));
    engine.state.players[1].library.push(test_instance(
        "putrid-library",
        extracted_definition.clone(),
        "player-1",
    ));
    engine.state.players[1].battlefield.push(test_instance(
        "putrid-battlefield",
        extracted_definition,
        "player-1",
    ));

    let source = test_instance(
        "surgical-extraction",
        test_definition("surgical-extraction", "Instant"),
        "player-0",
    );
    let stack_object = StackObject {
        id: "stack:surgical-extraction".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetNamedCard".to_string(),
            TargetRef::Card {
                instance_id: target.instance_id,
            },
        )]),
    };
    let effect = json!({
        "kind": "exileNamedCardsFromZones",
        "player": { "kind": "chosenTarget", "id": "targetNamedCard" },
        "chooser": { "kind": "controllerOf", "object": { "kind": "self" } },
        "nameFromCard": { "kind": "chosenTarget", "id": "targetNamedCard" },
        "zones": ["graveyard", "hand", "library"],
        "shuffleLibrary": true,
    });

    engine
        .execute_effect(
            &effect,
            None,
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("Surgical Extraction resolves");

    let exiled_ids = engine.state.players[1]
        .exile
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(exiled_ids.contains("putrid-graveyard"));
    assert!(exiled_ids.contains("putrid-hand"));
    assert!(exiled_ids.contains("putrid-library"));
    assert!(!exiled_ids.contains("putrid-battlefield"));
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .all(|card| card.instance_id != "putrid-graveyard")
    );
    assert!(
        engine.state.players[1]
            .hand
            .iter()
            .all(|card| card.instance_id != "putrid-hand")
    );
    assert!(
        engine.state.players[1]
            .library
            .iter()
            .all(|card| card.instance_id != "putrid-library")
    );
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|card| card.instance_id == "putrid-battlefield")
    );
    let note = engine
        .state
        .rule_modifiers
        .iter()
        .find(|modifier| modifier["kind"] == "observedCardsNote")
        .expect("searching the hidden zones records a private observation note");
    assert_eq!(note["viewerId"], "player-0");
    assert_eq!(note["playerId"], "player-1");
    assert_eq!(note["sourceName"], "surgical-extraction");
    assert_eq!(note["orderKnown"], false);
    assert!(
        note["zones"]
            .as_array()
            .is_some_and(|zones| zones.iter().any(|zone| {
                zone["zone"] == "library"
                    && zone["cards"].as_array().is_some_and(|cards| {
                        cards
                            .iter()
                            .any(|card| card["name"] == "Putrid Imp" && card["count"] == 1)
                    })
            }))
    );
}
