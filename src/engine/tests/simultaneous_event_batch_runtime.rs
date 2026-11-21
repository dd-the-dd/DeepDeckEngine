use super::*;

// Real Oracle text (Great Fierce Bee): the trigger groups one simultaneous
// event, which is one state-based-action pass (CR 603.2c, 704.3).
const ONE_OR_MORE_DIE: &str = "Whenever one or more other creatures die, scry 1.";

fn watcher() -> CardInstance {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Death watcher".to_string(),
        type_line: "Creature — Insect".to_string(),
        mana_cost: Some("{3}{U}".to_string()),
        oracle_text: Some(ONE_OR_MORE_DIE.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    assert_eq!(rule["event"]["aggregation"], "oneOrMore");
    let mut definition = test_definition("death-watcher", "Creature - Insect");
    definition.rules = vec![rule];
    test_instance("death-watcher", definition, "player-0")
}

fn admitted(engine: &mut GameEngine, mut card: CardInstance) -> CardInstance {
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn lethal_creature(engine: &mut GameEngine, id: &str) -> CardInstance {
    let mut card = admitted(
        engine,
        test_instance(id, test_definition(id, "Creature - Bear"), "player-1"),
    );
    card.damage_marked = 5;
    card
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn one_state_based_pass_is_one_event_and_separate_passes_are_two() {
    let mut engine = test_engine(2);
    let watcher = admitted(&mut engine, watcher());
    engine.state.players[0].battlefield = vec![watcher];

    // Two creatures die in the same pass: one trigger.
    let first = lethal_creature(&mut engine, "bear-1");
    let second = lethal_creature(&mut engine, "bear-2");
    engine.state.players[1].battlefield = vec![first, second];
    let stack = engine.state.stack.len();
    engine.check_state_based_actions();
    assert_eq!(engine.state.players[1].battlefield.len(), 0);
    assert_eq!(
        engine.state.stack.len(),
        stack + 1,
        "one event, one trigger"
    );

    // A later, separate death is a new event: a second trigger.
    let third = lethal_creature(&mut engine, "bear-3");
    engine.state.players[1].battlefield = vec![third];
    engine.check_state_based_actions();
    assert_eq!(
        engine.state.stack.len(),
        stack + 2,
        "a new event triggers again"
    );
}
