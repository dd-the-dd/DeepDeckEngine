//! A static ability's `condition` (CR 611.3a, "as long as") is evaluated
//! continuously with the source and controller: each query re-reads the
//! current game state, so the effect appears and disappears with it.
use super::*;

fn conditional_rule(condition: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "condition": condition,
        "modifiers": [
            {
                "kind": "modifyPowerToughness",
                "objects": { "kind": "self" },
                "power": 1,
                "toughness": 0,
            },
            { "kind": "grantKeyword", "objects": { "kind": "self" }, "keyword": "flying" },
        ],
    })
}

fn engine_with_source(condition: Value) -> GameEngine {
    let mut engine = test_engine(2);
    let mut definition = test_definition("conditional-source", "Creature - Test");
    definition.rules = vec![conditional_rule(condition)];
    engine.state.players[0].battlefield.push(test_instance(
        "conditional-source",
        definition,
        "player-0",
    ));
    engine
}

fn source_state(engine: &GameEngine) -> (bool, i32) {
    let source = engine.state.players[0].battlefield[0].clone();
    let power =
        GameEngine::effective_power_toughness(&engine.state)[&source.instance_id].current_power;
    (
        engine.permanent_has_effective_keyword(&source, "flying"),
        power,
    )
}

#[test]
fn graveyard_count_condition_toggles_keyword_and_power_with_the_state() {
    let mut engine = engine_with_source(json!({
        "kind": "compare",
        "operator": ">=",
        "left": {
            "kind": "countCards",
            "zone": { "kind": "graveyard", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            "where": null,
        },
        "right": { "kind": "integer", "value": 2 },
    }));
    assert_eq!(source_state(&engine), (false, 2), "condition not yet met");

    for id in ["card-a", "card-b"] {
        engine.state.players[0].graveyard.push(test_instance(
            id,
            test_definition(id, "Sorcery"),
            "player-0",
        ));
    }
    assert_eq!(
        source_state(&engine),
        (true, 3),
        "condition met: both modifiers apply"
    );

    engine.state.players[0].graveyard.pop();
    assert_eq!(
        source_state(&engine),
        (false, 2),
        "condition lapses: nothing is latched"
    );
}

#[test]
fn source_state_condition_is_evaluated_on_the_source_permanent() {
    let mut engine = engine_with_source(json!({
        "kind": "not",
        "operand": { "kind": "isTapped", "object": { "kind": "self" } },
    }));
    assert_eq!(source_state(&engine), (true, 3), "untapped source");
    engine.state.players[0].battlefield[0].tapped = true;
    assert_eq!(source_state(&engine), (false, 2), "tapped source");
}

#[test]
fn controller_event_history_condition_counts_only_the_current_turn() {
    let mut engine = engine_with_source(json!({
        "kind": "compare",
        "operator": ">=",
        "left": {
            "kind": "countEventsThisTurn",
            "event": "cardDrawn",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "where": null,
        },
        "right": { "kind": "integer", "value": 1 },
    }));
    assert_eq!(source_state(&engine), (false, 2));
    engine.state.players[0].library.push(test_instance(
        "drawn-card",
        test_definition("drawn-card", "Land"),
        "player-0",
    ));
    engine
        .draw_cards("player-0", 1)
        .expect("controller draws a card");
    assert_eq!(source_state(&engine), (true, 3));
    engine.state.turn_number += 1;
    assert_eq!(
        source_state(&engine),
        (false, 2),
        "history of an earlier turn does not count"
    );
}
