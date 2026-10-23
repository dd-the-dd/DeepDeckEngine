//! Real session execution; no controller, normal account, network or database.
use mtg_engine::engine::{CardDefinition, GameMode, GameSetup, PlayerDeck};
use mtg_engine::session::{CreateGameSessionRequest, GameSessionManager};
use serde_json::json;
use std::time::{Duration, Instant};

fn request(mode: GameMode, best_of: Option<u32>) -> CreateGameSessionRequest {
    let card = |id: String| CardDefinition {
        id,
        name: "Plains".into(),
        type_line: "Basic Land - Plains".into(),
        mana_cost: String::new(),
        power: None,
        toughness: None,
        rules: vec![],
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
    };
    let setup = GameSetup {
        players: (0..if mode == GameMode::Commander { 4 } else { 2 })
            .map(|seat| PlayerDeck {
                id: format!("p{seat}"),
                name: format!("Test {seat}"),
                starting_life: if mode == GameMode::Commander { 40 } else { 20 },
                cards: (0..if mode == GameMode::Commander { 100 } else { 60 })
                    .map(|index| {
                        let mut definition = card(format!("{seat}-{index}"));
                        if mode == GameMode::Commander && index == 99 {
                            definition.name = "Avacyn, Angel of Hope".into();
                            definition.type_line = "Legendary Creature - Angel".into();
                            definition.mana_cost = "{5}{W}{W}{W}".into();
                            definition.power = Some("8".into());
                            definition.toughness = Some("8".into());
                            definition.is_commander = true;
                        }
                        definition
                    })
                    .collect(),
            })
            .collect(),
        opening_hand_size: 7,
        starting_player: 0,
    };
    serde_json::from_value(json!({
        "setup":setup,"seed":7,"gameMode":mode,"bestOf":best_of,
        "maxTurns":200,"mulliganEnabled":false,"recordAnalytics":false
    }))
    .unwrap()
}

#[test]
fn best_of_defaults_and_invalid_values_are_checked_before_start() {
    let manager = GameSessionManager::new();
    for value in [0, 2, 4, 8, u32::MAX] {
        let error = manager
            .create(request(GameMode::Legacy, Some(value)))
            .unwrap_err();
        assert!(error.to_string().contains("bestOf"));
    }
    let error = manager
        .create(request(GameMode::Commander, Some(3)))
        .unwrap_err();
    assert!(error.to_string().contains("bestOf 1"));
    for best_of in [None, Some(1), Some(3)] {
        let initial = manager.create(request(GameMode::Legacy, best_of)).unwrap();
        assert_eq!(
            initial.match_state.as_ref().unwrap().games_to_win,
            if best_of == Some(1) { 1 } else { 2 }
        );
        manager.remove(&initial.session_id).unwrap();
    }
    for best_of in [None, Some(1)] {
        let initial = manager
            .create(request(GameMode::Commander, best_of))
            .unwrap();
        assert!(initial.match_state.is_none());
        assert_eq!(initial.state.players.len(), 4);
        manager.remove(&initial.session_id).unwrap();
    }
}

#[test]
fn legacy_one_and_three_are_actual_completed_series_not_just_subgame_status() {
    for best_of in [1, 3] {
        let manager = GameSessionManager::new();
        let initial = manager
            .create(request(GameMode::Legacy, Some(best_of)))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(180);
        loop {
            let view = manager.view(&initial.session_id).unwrap();
            assert!(view.error.is_none(), "{:?}", view.error);
            if let Some(series) = &view.match_state {
                if series.phase == "complete" {
                    let winner = series.winner_player_id.as_ref().unwrap();
                    assert_eq!(series.wins_by_player_id[winner], best_of / 2 + 1);
                    assert!(series.game_number >= best_of / 2 + 1);
                    if best_of == 1 {
                        assert_eq!(series.game_number, 1);
                    }
                    break;
                }
            }
            assert!(
                Instant::now() < until,
                "Series did not finish: {:?}",
                view.match_state
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        manager.remove(&initial.session_id).unwrap();
    }
}
