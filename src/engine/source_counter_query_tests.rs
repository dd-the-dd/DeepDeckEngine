use super::*;
use crate::engine::{CardDefinition, GameSetup, PlayerDeck, card_instance};
use serde_json::json;

#[test]
fn zero_source_counters_require_an_admitted_incarnation_not_a_same_id_guess() {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"counter-source", "name":"Counter Source", "typeLine":"Artifact", "manaCost":"{0}", "rules":[]
    })).unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 8],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        96271,
    )
    .unwrap();
    let mut source = card_instance("p0", &definition, 811);
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.state.players[0].battlefield.push(source.clone());
    let mut stack = StackObject {
        id: "counter-context".into(),
        controller: "p0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    let expression = json!({"kind":"countCounters","object":{"kind":"self"},"counter":"verse"});
    let eval = |engine: &GameEngine, stack: &StackObject| {
        engine.checked_runtime_numeric_expression(
            &expression,
            stack,
            &BTreeMap::new(),
            &BTreeMap::new(),
            "independentCounterCount",
        )
    };
    assert_eq!(eval(&engine, &stack).unwrap(), 0);
    // Fixture admission rejects negative counters. This deliberately malformed
    // internal state proves an all-counter sum cannot hide one behind positives.
    engine.state.players[0].battlefield[0]
        .counters
        .insert("bad".into(), -1);
    engine.state.players[0].battlefield[0]
        .counters
        .insert("good".into(), 2);
    assert!(
        engine
            .checked_runtime_numeric_expression(
                &json!({"kind":"countCounters","object":{"kind":"self"}}),
                &stack,
                &BTreeMap::new(),
                &BTreeMap::new(),
                "invalidCounterSum",
            )
            .is_err()
    );
    engine.state.players[0].battlefield[0].counters.clear();
    GameEngine::clear_tracked_zone_identity(&mut stack.card);
    assert!(eval(&engine, &stack).is_err());
    assert!(GameEngine::tracked_zone_identity(&stack.card).is_none());
    // An admitted snapshot with no actual departure capture also cannot become
    // a valid zero through the old snapshot fallback after its source vanished.
    stack.card = engine.state.players[0].battlefield[0].clone();
    engine.state.players[0].battlefield.clear();
    assert!(eval(&engine, &stack).is_err());
    stack
        .decisions
        .insert("sourceLkiDeparted".into(), json!(true));
    assert!(eval(&engine, &stack).is_err());
}
