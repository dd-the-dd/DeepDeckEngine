//! Real Oracle text -> primitive IR -> engine behavior for quantity
//! replacements of a player-performed event (CR 614.1a, 701.17a): the
//! `replaceEventQuantity` program of tokens and counters, over a mill.
use super::*;

fn parsed_static(name: &str, oracle: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Legendary Creature \u{2014} Human Advisor".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
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
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    let rule = parsed
        .abilities
        .into_iter()
        .find_map(|ability| ability.rule)
        .expect("one rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn library_cards(player: usize, count: usize) -> Vec<CardInstance> {
    (0..count)
        .map(|index| {
            let id = format!("library-{player}-{index}");
            test_instance(
                &id,
                test_definition(&id, "Instant"),
                &format!("player-{player}"),
            )
        })
        .collect()
}

fn engine_with(rule: Value) -> GameEngine {
    let mut engine = test_engine(2);
    let mut definition = test_definition("source", "Creature - Human Advisor");
    definition.rules = vec![rule];
    engine.state.players[0]
        .battlefield
        .push(test_instance("source", definition, "player-0"));
    engine.state.players[0].library = library_cards(0, 10);
    engine.state.players[1].library = library_cards(1, 10);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_opponents_mill_is_doubled_and_the_controllers_is_not() {
    // Bruvac the Grandiloquent, with its mill reminder.
    let rule = parsed_static(
        "Bruvac the Grandiloquent",
        "If an opponent would mill one or more cards, they mill twice that many cards instead. (To mill a card, a player puts the top card of their library into their graveyard.)",
    );
    assert_eq!(rule["modifiers"][0]["event"]["kind"], "mill", "{rule:#}");
    let mut engine = engine_with(rule);
    let milled = engine
        .mill_cards("player-1", 2, Some("player-0"))
        .expect("the opponent mills");
    assert_eq!(milled.len(), 4);
    assert_eq!(engine.state.players[1].graveyard.len(), 4);
    let milled = engine
        .mill_cards("player-0", 2, Some("player-0"))
        .expect("the controller mills");
    assert_eq!(milled.len(), 2, "the controller is not an opponent");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_postposed_offset_adds_to_the_milled_quantity() {
    // The Water Crystal: `that many cards plus four`.
    let rule = parsed_static(
        "The Water Crystal",
        "If an opponent would mill one or more cards, they mill that many cards plus four instead.",
    );
    let mut engine = engine_with(rule);
    let milled = engine
        .mill_cards("player-1", 1, Some("player-0"))
        .expect("the opponent mills");
    assert_eq!(milled.len(), 5);
}
