#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn echo_is_due_after_entry_then_is_paid_or_sacrificed_only_once() {
    let echo_rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "stepBegan",
            "step": "upkeep",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
        "condition": { "kind": "sourceNeedsEcho" },
        "effects": [{ "kind": "payManaOrSacrificeSource", "manaCost": "{W}" }],
    });

    let mut paid_engine = test_engine(2);
    let mut paid_definition = test_definition("paid-echo", "Creature - Spirit");
    paid_definition.rules = vec![echo_rule.clone()];
    let mut paid_echo = test_instance("paid-echo", paid_definition, "player-0");
    paid_engine
        .apply_enter_replacements(&mut paid_echo, &mut EmeritusDecisionProvider)
        .expect("echo entry state is installed");
    paid_engine.state.players[0].battlefield.push(paid_echo);
    paid_engine.state.players[0].mana_pool.push(FloatingMana {
        symbol: "W".to_string(),
        spend_restriction: None,
    });
    paid_engine.enqueue_step_triggers("upkeep");
    assert_eq!(paid_engine.state.stack.len(), 1);
    paid_engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("echo payment resolves");
    assert!(paid_engine.permanent_position("paid-echo").is_some());
    assert!(paid_engine.state.players[0].mana_pool.is_empty());
    paid_engine.enqueue_step_triggers("upkeep");
    assert!(paid_engine.state.stack.is_empty());

    let mut declined_engine = test_engine(2);
    let mut declined_definition = test_definition("declined-echo", "Creature - Spirit");
    declined_definition.rules = vec![echo_rule];
    let mut declined_echo = test_instance("declined-echo", declined_definition, "player-0");
    declined_engine
        .apply_enter_replacements(&mut declined_echo, &mut EmeritusDecisionProvider)
        .expect("echo entry state is installed");
    declined_engine.state.players[0]
        .battlefield
        .push(declined_echo);
    declined_engine.enqueue_step_triggers("upkeep");
    declined_engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("declined echo resolves");
    assert!(
        declined_engine
            .permanent_position("declined-echo")
            .is_none()
    );
    assert!(
        declined_engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "declined-echo")
    );
}
