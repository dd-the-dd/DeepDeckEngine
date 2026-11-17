use super::super::*;
use super::*;
use rand::RngCore;

fn game() -> GameEngine {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        10945,
    )
    .unwrap();
    for index in 0..1024 {
        engine.record_event("retainedHistory", None, None, json!({"index":index}));
    }
    engine
}

#[test]
fn checkpoint_keeps_history_visible_without_copying_its_prefix() {
    let mut engine = game();
    let history = engine.state.events.clone();
    let pointer = engine.state.events.as_ptr();
    let checkpoint = ResolutionCheckpoint::capture(&mut engine);
    assert!(checkpoint.snapshot.state.events.is_empty());
    assert_eq!(checkpoint.snapshot.state.events.capacity(), 0);
    assert_eq!(engine.state.events, history);
    assert_eq!(engine.state.events.as_ptr(), pointer);
    drop(checkpoint);
    assert_eq!(engine.state.events, history);
}

#[test]
fn successful_transaction_keeps_prior_history_and_appends_its_events() {
    let mut engine = game();
    let history = engine.state.events.clone();
    engine
        .with_replacement_checkpoint(true, |engine| {
            assert_eq!(engine.state.events, history);
            engine.state.players[0].life += 3;
            engine.record_event("newHistory", None, None, json!({"count":3}));
            Ok(())
        })
        .unwrap();
    assert_eq!(&engine.state.events[..history.len()], history.as_slice());
    assert_eq!(engine.state.events.len(), history.len() + 1);
    assert_eq!(engine.state.players[0].life, 23);
}

#[test]
fn failure_restores_original_history_totals_and_rng() {
    let mut engine = game();
    let before = serde_json::to_value(&engine.state).unwrap();
    let mut rng_before = engine.rng.clone();
    let result: Result<(), EngineError> = engine.with_replacement_checkpoint(true, |engine| {
        engine.state.players[0].life -= 2;
        engine.rng.next_u64();
        engine.record_event("discardOnRollback", None, None, json!({"flag":true}));
        Err(EngineError::new("deliberate checkpoint test refusal"))
    });
    assert!(result.is_err());
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    assert_eq!(engine.rng.next_u64(), rng_before.next_u64());
}

#[test]
fn swallowed_nested_failure_still_restores_the_outer_history_prefix() {
    let mut engine = game();
    let before = serde_json::to_value(&engine.state).unwrap();
    let result: Result<(), EngineError> = engine.with_replacement_checkpoint(true, |engine| {
        engine.record_event("outerAppend", None, None, json!({}));
        let nested: Result<(), EngineError> = engine.with_replacement_checkpoint(true, |engine| {
            engine.state.players[1].life -= 4;
            engine.record_event("innerAppend", None, None, json!({}));
            Err(EngineError::new("nested refusal"))
        });
        assert!(nested.is_err());
        engine.record_event("afterCaughtFailure", None, None, json!({}));
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[test]
fn position_reset_cannot_erase_history_inside_a_transaction() {
    let mut engine = game();
    let position = DeterministicGamePosition {
        turn_number: 1,
        active_player_id: "p0".into(),
        step: GameStep::PrecombatMain,
        players: (0..2)
            .map(|i| DeterministicPlayerPosition {
                player_id: format!("p{i}"),
                life: 20,
                battlefield_definition_ids: vec![],
                hand_definition_ids: vec![],
                library_size: 0,
                mana_pool: vec![],
            })
            .collect(),
    };
    let before = serde_json::to_value(&engine.state).unwrap();
    let result = engine.with_replacement_checkpoint(true, |engine| {
        engine.configure_deterministic_position(&position)
    });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("active transaction")
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    engine.configure_deterministic_position(&position).unwrap();
    assert!(engine.state.events.len() < 1024);
}
