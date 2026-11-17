use super::*;

fn early_turn_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Early Land".to_string(),
        type_line: "Land".to_string(),
        mana_cost: None,
        oracle_text: Some(
            "This land enters tapped unless it's your first, second, or third turn of the game."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    parsed.abilities[0]
        .rule
        .clone()
        .expect("the early-turn entry replacement should parse")
}

fn entering_land(rule: Value) -> CardInstance {
    let mut definition = test_definition("test-early-land", "Land");
    definition.rules = vec![rule];
    test_instance("test-early-land", definition, "player-1")
}

fn engine_with_turn_starts(turn_numbers: &[u32], active_player: usize) -> GameEngine {
    let mut engine = test_engine(3);
    engine.state.events.clear();
    for (index, turn_number) in turn_numbers.iter().copied().enumerate() {
        engine.state.events.push(GameEvent {
            sequence: index + 1,
            turn_number,
            step: GameStep::Untap,
            kind: "turnStarted".to_string(),
            player_id: Some("player-1".to_string()),
            card_instance_id: None,
            detail: json!({}),
        });
    }
    engine.state.turn_number = turn_numbers.last().copied().unwrap_or(1);
    engine.state.active_player = active_player;
    engine
}

fn apply_entry_replacement(engine: &mut GameEngine) -> CardInstance {
    let rule = early_turn_rule();
    assert!(rule_is_executable(&rule));
    let mut land = entering_land(rule);
    engine
        .apply_enter_replacements(&mut land, &mut EmeritusDecisionProvider)
        .expect("the early-turn replacement should resolve");
    land
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn early_turn_entry_counts_the_current_turn_and_respects_multiplayer_turn_ownership() {
    let mut first_turn = test_engine(3);
    first_turn.state.active_player = 1;
    first_turn.state.events.clear();
    first_turn
        .start_turn(&mut EmeritusDecisionProvider)
        .expect("the first turn should start");
    assert_eq!(
        first_turn
            .state
            .events
            .iter()
            .filter(|event| event.kind == "turnStarted"
                && event.player_id.as_deref() == Some("player-1"))
            .count(),
        1,
        "start_turn records the current turn before a land can enter in the main phase"
    );
    assert!(
        !apply_entry_replacement(&mut first_turn).tapped,
        "turnStarted is already recorded when a land enters during the first main phase"
    );

    let mut third_turn = engine_with_turn_starts(&[2, 5, 8], 1);
    assert!(!apply_entry_replacement(&mut third_turn).tapped);

    let mut opponents_turn = engine_with_turn_starts(&[2, 5], 0);
    assert!(
        apply_entry_replacement(&mut opponents_turn).tapped,
        "having taken fewer than three turns is insufficient during another player's turn"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn extra_turns_are_included_in_the_players_turn_count() {
    let mut after_extra_turn = engine_with_turn_starts(&[2, 5, 6, 7], 1);
    assert!(
        apply_entry_replacement(&mut after_extra_turn).tapped,
        "the fourth turnStarted event counts even when prior turns were consecutive extra turns"
    );
}
