//! `for the first time each turn` (Attended Healer, Vengeful Warchief): the
//! first occurrence of the event this turn triggers once; a later one does
//! not, and an occurrence before the observer arrived still counts.
use super::*;
use serde_json::json;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules,
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", Vec::new()); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        603,
    )
    .expect("first occurrence test setup is valid")
}

fn first_life_gain_rule(effect_count: i64) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "lifeGained",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "firstTimeEachTurn": true,
        },
        "effects": [{
            "kind": "drawCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": { "kind": "integer", "value": effect_count },
        }],
    })
}

fn add_observer(engine: &mut GameEngine, rules: Vec<Value>) {
    let mut card = card_instance("player-0", &definition("healer", "Creature", rules), 0);
    card.instance_id = "healer".to_string();
    card.printed_definition = Some(card.definition.clone());
    engine.state.players[0].battlefield.push(card);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

fn triggered_count(engine: &GameEngine) -> usize {
    engine
        .state
        .stack
        .iter()
        .filter(|object| object.ability_kind.as_deref() == Some("triggeredAbility"))
        .count()
}

#[test]
fn the_first_life_gain_of_the_turn_triggers_once() {
    let mut engine = engine();
    add_observer(&mut engine, vec![first_life_gain_rule(1)]);
    engine.gain_life("player-0", 2).expect("first gain");
    assert_eq!(triggered_count(&engine), 1);
    engine.gain_life("player-0", 3).expect("second gain");
    assert_eq!(triggered_count(&engine), 1, "only the first time each turn");
    // Another player's gain is not `you gain life`.
    engine.gain_life("player-1", 1).expect("opponent gain");
    assert_eq!(triggered_count(&engine), 1);
}

#[test]
fn a_gain_before_the_observer_arrived_was_the_first_one() {
    let mut engine = engine();
    engine
        .gain_life("player-0", 2)
        .expect("gain without observer");
    add_observer(&mut engine, vec![first_life_gain_rule(1)]);
    engine.gain_life("player-0", 2).expect("second gain");
    assert_eq!(triggered_count(&engine), 0);
}

#[test]
fn two_rules_with_the_same_event_each_trigger_once() {
    // Gourmand's Talent: two class levels observe the same first gain.
    let mut engine = engine();
    add_observer(
        &mut engine,
        vec![first_life_gain_rule(1), first_life_gain_rule(2)],
    );
    engine.gain_life("player-0", 1).expect("first gain");
    assert_eq!(triggered_count(&engine), 2);
    engine.gain_life("player-0", 1).expect("second gain");
    assert_eq!(triggered_count(&engine), 2);
}
