use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

#[test]
fn target_head_is_shared_with_the_target_child_and_excludes_collections() {
    for (text, expected) in [
        ("target creature", true),
        ("another target creature", true),
        ("up to one other target creature", true),
        ("two target creatures", true),
        ("all graveyards", false),
        ("each opponent's graveyard", false),
        ("a creature", false),
    ] {
        let tokens = recognize_oracle_entities(text);
        let cursor = OracleTokenCursor::new("shared-target-head-test", &tokens);
        assert_eq!(starts_explicit_target_slot(&cursor), expected, "{text}");
    }
}

#[test]
fn possessive_target_player_zone_is_not_an_object_target_slot() {
    // `target player's` selects a zone owner, not an object target slot.
    for (text, expected) in [
        ("target player's graveyard", true),
        ("target opponent's graveyard", true),
        ("target player", false),
        ("target creature", false),
    ] {
        let tokens = recognize_oracle_entities(text);
        let cursor = OracleTokenCursor::new("possessive-player-head-test", &tokens);
        assert_eq!(starts_possessive_target_player(&cursor), expected, "{text}");
    }
}
