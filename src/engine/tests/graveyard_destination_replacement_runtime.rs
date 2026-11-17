use super::*;

fn replacement_source(oracle_text: &str) -> CardInstance {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Graveyard Seal".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    let mut definition = test_definition("test-graveyard-seal", "Enchantment");
    definition.rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    assert!(definition.rules.iter().all(rule_is_executable));
    test_instance("test-graveyard-seal", definition, "player-0")
}

fn test_token(instance_id: &str, owner: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, "Token Creature - Spirit");
    definition.is_token = true;
    test_instance(instance_id, definition, owner)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn global_replacement_exiles_cards_for_every_player_and_tokens_before_they_cease() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(replacement_source(
        "When this enchantment enters, exile all graveyards.\nIf a card or token would be put into a graveyard from anywhere, exile it instead.",
    ));

    for (player_index, instance_id) in [(0, "controller-card"), (1, "opponent-card")] {
        let card = test_instance(
            instance_id,
            test_definition(instance_id, "Sorcery"),
            &format!("player-{player_index}"),
        );
        assert!(!engine.put_card_into_graveyard(player_index, card));
        assert!(
            engine.state.players[player_index]
                .exile
                .iter()
                .any(|card| card.instance_id == instance_id)
        );
    }

    engine.state.players[1].library = vec![test_instance(
        "milled-card",
        test_definition("milled-card", "Sorcery"),
        "player-1",
    )];
    assert_eq!(
        engine
            .mill_cards("player-1", 1, Some("player-0"))
            .expect("milling should use the central graveyard destination"),
        vec!["milled-card".to_string()]
    );
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "milled-card")
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "cardMilled" && event.card_instance_id.as_deref() == Some("milled-card")
    }));

    engine.state.players[1]
        .battlefield
        .push(test_token("opponent-token", "player-1"));
    engine
        .move_permanent_to_graveyard("opponent-token", "destroyed")
        .expect("the token destination should be replaced");
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "opponent-token")
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "graveyardDestinationReplaced"
            && event.card_instance_id.as_deref() == Some("opponent-token")
    }));

    engine.check_state_based_actions();
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .all(|card| card.instance_id != "opponent-token")
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "tokenCeasedToExist"
            && event.card_instance_id.as_deref() == Some("opponent-token")
            && event.detail["zone"] == "exile"
    }));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn card_only_opponent_replacement_preserves_both_scope_and_token_filter() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(replacement_source(
        "If a card would be put into an opponent's graveyard from anywhere, exile it instead.",
    ));

    let controller_card = test_instance(
        "controller-card",
        test_definition("controller-card", "Sorcery"),
        "player-0",
    );
    assert!(engine.put_card_into_graveyard(0, controller_card));
    let opponent_card = test_instance(
        "opponent-card",
        test_definition("opponent-card", "Sorcery"),
        "player-1",
    );
    assert!(!engine.put_card_into_graveyard(1, opponent_card));

    engine.state.players[1]
        .battlefield
        .push(test_token("opponent-token", "player-1"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .move_permanent_to_graveyard("opponent-token", "destroyed")
        .expect("card-only replacement should leave a token in the graveyard");
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "opponent-token")
    );
    assert!(!engine.state.events.iter().any(|event| {
        event.kind == "graveyardDestinationReplaced"
            && event.card_instance_id.as_deref() == Some("opponent-token")
    }));
}
