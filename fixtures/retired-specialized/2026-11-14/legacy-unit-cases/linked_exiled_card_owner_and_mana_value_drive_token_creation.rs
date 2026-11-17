#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn linked_exiled_card_owner_and_mana_value_drive_token_creation() {
    let mut engine = test_engine(2);
    let source = test_instance(
        "linked-source",
        test_definition("linked-source", "Creature - Test"),
        "player-0",
    );
    let mut victim_definition = test_definition("linked-victim", "Artifact Creature - Test");
    victim_definition.mana_cost = "{2}{W}".to_string();
    let victim = test_instance("linked-victim", victim_definition, "player-1");
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[1].battlefield.push(victim);
    let stack_object = StackObject {
        id: "stack:linked-variable-token".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "exileTarget".to_string(),
            TargetRef::Permanent {
                instance_id: "linked-victim".to_string(),
            },
        )]),
    };
    let exile = json!({
        "kind": "exilePermanentWithSource",
        "permanent": { "kind": "chosenTarget", "id": "exileTarget" },
        "candidates": {
            "kind": "permanents",
            "where": { "kind": "cardTypeContains", "value": "Artifact" },
        },
        "minimum": 1,
        "maximum": 1,
        "returnWhenSourceLeaves": false,
    });
    let linked_card = json!({ "kind": "cardExiledWithSource" });
    let mana_value = json!({
        "kind": "manaValueOf",
        "object": linked_card.clone(),
    });
    let create = json!({
        "kind": "createTokens",
        "controller": { "kind": "ownerOf", "object": linked_card },
        "quantity": { "kind": "integer", "value": 1 },
        "token": {
            "colors": ["blue"],
            "types": ["Creature"],
            "subtypes": ["Illusion"],
            "power": mana_value.clone(),
            "toughness": mana_value,
            "abilities": [],
        },
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
        .expect("the source links the exiled permanent");
    engine
        .execute_effect(
            &create,
            None,
            &stack_object,
            &mut bindings,
            &mut decisions,
            &mut provider,
        )
        .expect("the linked card creates the variable token");

    let token = engine.state.players[1]
        .battlefield
        .iter()
        .find(|card| card.definition.is_token)
        .expect("the exiled card's owner receives the token");
    assert!(token.definition.type_line.contains("Illusion"));
    assert_eq!(token.definition.power.as_deref(), Some("3"));
    assert_eq!(token.definition.toughness.as_deref(), Some("3"));
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|card| !card.definition.is_token)
    );
}
