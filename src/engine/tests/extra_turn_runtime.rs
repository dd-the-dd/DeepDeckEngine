//! Runtime proof of turn succession with extra turns (CR 500.7): the player
//! an effect gives an extra turn takes the next turn even when another
//! player is active, the most recently created extra turn is taken first,
//! and older extra turns of the same turn follow it.
use super::*;

fn three_player_engine() -> GameEngine {
    let filler: CardDefinition = serde_json::from_value(json!({
        "id": "filler", "name": "filler", "typeLine": "Basic Land — Plains",
    }))
    .expect("standalone test definition is valid");
    GameEngine::new(
        GameSetup {
            players: (0..3)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        500,
    )
    .expect("standalone test engine is valid")
}

fn grant_extra_turn(engine: &mut GameEngine, player_id: &str) {
    engine.state.rule_modifiers.push(json!({
        "kind": "extraTurnForPlayer",
        "playerId": player_id,
        "afterTurn": engine.state.turn_number,
    }));
}

fn pending_extra_turns(engine: &GameEngine) -> usize {
    engine
        .state
        .rule_modifiers
        .iter()
        .filter(|modifier| value_kind(modifier) == Some("extraTurnForPlayer"))
        .count()
}

#[test]
fn an_extra_turn_given_to_a_non_active_player_is_taken_next() {
    let mut engine = three_player_engine();
    engine.state.active_player = 0;
    grant_extra_turn(&mut engine, "player-2");
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 2);
    assert_eq!(pending_extra_turns(&engine), 0);
}

#[test]
fn the_most_recent_extra_turn_is_taken_first_and_the_older_one_follows() {
    let mut engine = three_player_engine();
    engine.state.active_player = 0;
    grant_extra_turn(&mut engine, "player-0");
    grant_extra_turn(&mut engine, "player-1");
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 1);
    assert_eq!(pending_extra_turns(&engine), 1);
    engine.state.turn_number += 1;
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 0);
    assert_eq!(pending_extra_turns(&engine), 0);
    // Turn order then continues from the player who took the extra turn.
    engine.state.turn_number += 1;
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 1);
}

#[test]
fn without_an_extra_turn_the_next_live_player_takes_the_turn() {
    let mut engine = three_player_engine();
    engine.state.active_player = 0;
    // An extra turn granted after an earlier turn is not this turn's.
    engine.state.rule_modifiers.push(json!({
        "kind": "extraTurnForPlayer",
        "playerId": "player-2",
        "afterTurn": engine.state.turn_number + 5,
    }));
    engine.state.players[1].has_lost = true;
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 2);
    assert_eq!(pending_extra_turns(&engine), 1);
}
