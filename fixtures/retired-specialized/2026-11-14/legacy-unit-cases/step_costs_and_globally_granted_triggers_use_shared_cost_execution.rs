#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1"))]
#[test]
fn step_costs_and_globally_granted_triggers_use_shared_cost_execution() {
    let mut paid_engine = test_engine(2);
    let mut prison_definition = test_definition("energy-prison", "Enchantment");
    prison_definition.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "stepBegan",
            "step": "precombatMain",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
        "effects": [{
            "kind": "payCostOrMoveSource",
            "cost": {
                "kind": "payPlayerCounters",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "counter": "energy",
                "count": { "kind": "integer", "value": 1 },
            },
            "otherwise": "sacrifice",
        }],
    })];
    paid_engine.state.players[0]
        .counters
        .insert("energy".to_string(), 1);
    paid_engine.state.players[0].battlefield.push(test_instance(
        "energy-prison",
        prison_definition.clone(),
        "player-0",
    ));
    paid_engine.enqueue_step_triggers("precombatMain");
    paid_engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the available energy payment keeps the source");
    assert!(paid_engine.permanent_position("energy-prison").is_some());
    assert_eq!(paid_engine.state.players[0].counters["energy"], 0);

    let mut unpaid_engine = test_engine(2);
    unpaid_engine.state.players[0]
        .battlefield
        .push(test_instance(
            "unpaid-prison",
            prison_definition,
            "player-0",
        ));
    unpaid_engine.enqueue_step_triggers("precombatMain");
    unpaid_engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("an unavailable energy payment sacrifices the source");
    assert!(unpaid_engine.permanent_position("unpaid-prison").is_none());

    let mut granted_engine = test_engine(2);
    let granted_upkeep = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "stepBegan",
            "step": "upkeep",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
        "effects": [{
            "kind": "payCostOrMoveSource",
            "cost": { "kind": "payMana", "manaCost": "{1}" },
            "otherwise": "destroy",
        }],
    });
    let mut grant_definition = test_definition("global-grant", "Land");
    grant_definition.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "grantTriggeredAbility",
            "objects": {
                "kind": "permanents",
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            },
            "ability": granted_upkeep,
        }],
    })];
    granted_engine.state.players[0]
        .battlefield
        .push(test_instance("global-grant", grant_definition, "player-0"));
    granted_engine.state.players[0]
        .battlefield
        .push(test_instance(
            "paid-creature",
            test_definition("paid-creature", "Creature"),
            "player-0",
        ));
    granted_engine.state.players[1]
        .battlefield
        .push(test_instance(
            "destroyed-creature",
            test_definition("destroyed-creature", "Creature"),
            "player-1",
        ));
    let granted_rule = &granted_engine.state.players[0].battlefield[0]
        .definition
        .rules[0];
    assert!(
        battlefield_static_rule_supported(granted_rule),
        "the global granted ability must be a live static modifier: {granted_rule}"
    );
    assert!(triggered_rule_supported(
        &granted_rule["modifiers"][0]["ability"]
    ));
    assert_eq!(
        granted_engine
            .battlefield_static_modifiers()
            .into_iter()
            .filter(|(_, modifier)| value_kind(modifier) == Some("grantTriggeredAbility"))
            .count(),
        1
    );
    granted_engine.state.players[0]
        .mana_pool
        .push(FloatingMana {
            symbol: "C".to_string(),
            spend_restriction: None,
        });
    granted_engine.enqueue_step_triggers("upkeep");
    assert_eq!(granted_engine.state.stack.len(), 1);
    granted_engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the active player's creature pays its granted upkeep");
    assert!(granted_engine.permanent_position("paid-creature").is_some());

    granted_engine.state.active_player = 1;
    granted_engine.enqueue_step_triggers("upkeep");
    assert_eq!(granted_engine.state.stack.len(), 1);
    granted_engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the other creature is destroyed when its granted cost is unpaid");
    assert!(
        granted_engine
            .permanent_position("destroyed-creature")
            .is_none()
    );
}
