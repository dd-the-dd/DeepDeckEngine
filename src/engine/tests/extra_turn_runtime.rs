//! Runtime proof of turn succession with extra turns (CR 500.7): the player
//! an effect gives an extra turn takes the next turn even when another
//! player is active, the most recently created extra turn is taken first,
//! and older extra turns of the same turn follow it. Once the chain of extra
//! turns ends, the player due after the interrupted turn takes the next one.
use super::*;

fn three_player_engine() -> GameEngine {
    engine_with_players(3)
}

fn engine_with_players(player_count: usize) -> GameEngine {
    let filler: CardDefinition = serde_json::from_value(json!({
        "id": "filler", "name": "filler", "typeLine": "Basic Land — Plains",
    }))
    .expect("standalone test definition is valid");
    GameEngine::new(
        GameSetup {
            players: (0..player_count)
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
    // Turn order then resumes after player 0's original turn.
    engine.state.turn_number += 1;
    engine.advance_to_next_turn_player();
    assert_eq!(engine.state.active_player, 1);
}

/// Takes the turns following the current one and returns their active players.
fn following_turns(engine: &mut GameEngine, count: usize) -> Vec<usize> {
    (0..count)
        .map(|_| {
            engine.advance_to_next_turn_player();
            engine.state.turn_number += 1;
            engine.state.active_player
        })
        .collect()
}

#[test]
fn two_players_an_opponent_extra_turn_is_followed_by_their_normal_turn() {
    // Player 1's turn gives player 0 an extra turn: 1, 0 (extra), 0, 1.
    let mut engine = engine_with_players(2);
    engine.state.active_player = 1;
    grant_extra_turn(&mut engine, "player-0");
    assert_eq!(following_turns(&mut engine, 3), vec![0, 0, 1]);
    assert!(engine.state.rule_modifiers.is_empty());
}

#[test]
fn three_players_an_extra_turn_does_not_skip_the_next_player() {
    // Player 0 gives player 2 an extra turn: 0, 2 (extra), 1, 2, 0.
    let mut engine = three_player_engine();
    engine.state.active_player = 0;
    grant_extra_turn(&mut engine, "player-2");
    assert_eq!(following_turns(&mut engine, 4), vec![2, 1, 2, 0]);
    assert!(engine.state.rule_modifiers.is_empty());
}

#[test]
fn an_extra_turn_granted_during_an_extra_turn_keeps_the_original_resume_player() {
    // Player 0 gives player 2 an extra turn, during which player 2 grants
    // itself another one: 0, 2 (extra), 2 (extra), 1.
    let mut engine = three_player_engine();
    engine.state.active_player = 0;
    grant_extra_turn(&mut engine, "player-2");
    assert_eq!(following_turns(&mut engine, 1), vec![2]);
    grant_extra_turn(&mut engine, "player-2");
    assert_eq!(following_turns(&mut engine, 2), vec![2, 1]);
    assert!(engine.state.rule_modifiers.is_empty());
}

#[test]
fn the_resume_player_skips_a_player_who_lost_during_the_extra_turn() {
    let mut engine = three_player_engine();
    engine.state.active_player = 0;
    grant_extra_turn(&mut engine, "player-2");
    assert_eq!(following_turns(&mut engine, 1), vec![2]);
    engine.state.players[1].has_lost = true;
    assert_eq!(following_turns(&mut engine, 1), vec![2]);
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
