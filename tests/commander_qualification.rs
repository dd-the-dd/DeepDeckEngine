//! Small integration target: do not build the enormous all-library-test binary
//! just to validate the HTTP table policy on memory-constrained Player machines.
use mtg_engine::engine::GameMode;
use mtg_engine::http::validate_game_mode_player_count;

#[test]
fn ordinary_commander_tables_are_still_four_and_short_pods_are_explicit() {
    for seats in 0..=8 {
        assert_eq!(
            validate_game_mode_player_count(GameMode::Commander, seats, false).is_ok(),
            seats == 4
        );
        assert_eq!(
            validate_game_mode_player_count(GameMode::Commander, seats, true).is_ok(),
            (2..=4).contains(&seats)
        );
    }
}

#[test]
fn short_pod_opt_in_cannot_silently_select_legacy_or_duel_commander() {
    for mode in [GameMode::Legacy, GameMode::DuelCommander, GameMode::Free] {
        for seats in 0..=5 {
            assert!(validate_game_mode_player_count(mode, seats, true).is_err());
        }
    }
    assert!(validate_game_mode_player_count(GameMode::Legacy, 2, false).is_ok());
    assert!(validate_game_mode_player_count(GameMode::DuelCommander, 2, false).is_ok());
    assert!(validate_game_mode_player_count(GameMode::DuelCommander, 4, false).is_err());
}
