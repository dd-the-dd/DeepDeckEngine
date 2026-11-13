use super::*;

fn creature(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature — Test");
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    test_instance(id, definition, controller)
}

#[test]
fn exalted_uses_generic_temporary_stat_change_only_for_one_attacker() {
    let mut hierarch_definition = test_definition("ignoble-hierarch", "Creature — Goblin Shaman");
    hierarch_definition.power = Some("0".to_string());
    hierarch_definition.toughness = Some("1".to_string());
    hierarch_definition.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "exalted" },
    }));

    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance("ignoble-hierarch", hierarch_definition, "player-0"),
        creature("lone-attacker", "player-0"),
        creature("second-attacker", "player-0"),
    ];

    engine.enqueue_declared_attacker_triggers(&["lone-attacker".to_string()]);
    assert_eq!(engine.state.stack.len(), 1);
    let trigger = engine.state.stack.last().expect("exalted trigger exists");
    let effect = &trigger.ability_rule.as_ref().expect("trigger has a rule")["effects"][0];
    assert_eq!(value_kind(effect), Some("modifyPowerToughness"));
    assert_eq!(value_kind(&effect["object"]), Some("triggeringPermanent"));
    assert_eq!(
        value_kind(&effect["duration"]),
        Some("untilEndOfCurrentTurn")
    );
    assert!(
        !trigger
            .ability_rule
            .as_ref()
            .expect("trigger has a rule")
            .to_string()
            .contains("resolveExalted")
    );

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("exalted trigger resolves");
    let (player_index, card_index) = engine
        .permanent_position("lone-attacker")
        .expect("lone attacker remains on the battlefield");
    let attacker = &engine.state.players[player_index].battlefield[card_index];
    assert_eq!(engine.current_power(attacker), 3);
    assert_eq!(engine.current_toughness(attacker), 3);
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "powerToughnessModified")
            .count(),
        1
    );

    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("end-of-turn cleanup succeeds");
    let (player_index, card_index) = engine
        .permanent_position("lone-attacker")
        .expect("lone attacker remains after cleanup");
    let attacker = &engine.state.players[player_index].battlefield[card_index];
    assert_eq!(engine.current_power(attacker), 2);
    assert_eq!(engine.current_toughness(attacker), 2);

    engine.enqueue_declared_attacker_triggers(&[
        "lone-attacker".to_string(),
        "second-attacker".to_string(),
    ]);
    assert!(engine.state.stack.is_empty());
}
