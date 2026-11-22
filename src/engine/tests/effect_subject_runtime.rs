//! Real Oracle text -> primitive IR -> engine behavior for effect sentences
//! with an explicit subject (docs/oracle-clause-layer.md, phase 6): the
//! player the subject names is the one the action changes.
use super::*;

fn parsed_rules(type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Subject Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn resolve_stack(engine: &mut GameEngine) {
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the triggered ability resolves");
    }
}

fn counters(engine: &GameEngine, player: usize, counter: &str) -> i64 {
    engine.state.players[player]
        .counters
        .get(counter)
        .copied()
        .map_or(0, i64::from)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn player_counters_go_to_the_player_the_subject_names() {
    // Unused recombinations of Pit Scorpion and Aethertorch Renegade: the
    // event's player gets the poison counter, the controller the energy.
    let mut rules = parsed_rules(
        "Enchantment",
        "Whenever an opponent draws a card, that player gets a poison counter.",
    );
    rules.extend(parsed_rules(
        "Enchantment",
        "Whenever an opponent draws a card, you get {E}{E} (two energy counters).",
    ));
    let mut definition = test_definition("subject", "Enchantment");
    definition.rules = rules;
    let mut engine = test_engine(3);
    engine.state.players[0]
        .battlefield
        .push(test_instance("subject", definition, "player-0"));
    engine.draw_cards("player-2", 1).expect("player-2 draws");
    resolve_stack(&mut engine);
    assert_eq!(counters(&engine, 2, "poison"), 1, "the drawing opponent");
    assert_eq!(counters(&engine, 1, "poison"), 0);
    assert_eq!(counters(&engine, 0, "energy"), 2, "the controller");
    assert_eq!(counters(&engine, 2, "energy"), 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn they_is_the_player_the_event_records() {
    // Sheoldred, the Apocalypse's opponent clause: the drawing opponent loses
    // the life, not the controller nor the other opponent.
    let rules = parsed_rules(
        "Creature \u{2014} Phyrexian Praetor",
        "Whenever an opponent draws a card, they lose 2 life.",
    );
    let mut definition = test_definition("sheoldred", "Creature");
    definition.rules = rules;
    let mut engine = test_engine(3);
    engine.state.players[0]
        .battlefield
        .push(test_instance("sheoldred", definition, "player-0"));
    engine.draw_cards("player-1", 1).expect("player-1 draws");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[1].life, 18, "the drawing opponent");
    assert_eq!(engine.state.players[2].life, 20);
    assert_eq!(engine.state.players[0].life, 20);
}
