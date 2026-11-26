use crate::oracle::syntax::recognize_entities;

use super::super::{EffectDurationNode, LocalPlayerReference};
use super::{OracleTokenCursor, recognize_effect_duration};

#[test]
fn current_and_next_turn_durations_are_distinct_typed_values() {
    for (text, expected) in [
        ("this turn", EffectDurationNode::UntilEndOfCurrentTurn),
        (
            "until end of turn",
            EffectDurationNode::UntilEndOfCurrentTurn,
        ),
        (
            "until your next turn",
            EffectDurationNode::UntilNextTurn {
                player: LocalPlayerReference::Controller,
            },
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("duration-test", &tokens);
        assert_eq!(
            recognize_effect_duration(&mut cursor).expect("duration parses"),
            expected
        );
        assert_eq!(cursor.position(), tokens.len());
    }
}

/// The indefinite duration is read where a clause ends without one, at any
/// position: a cursor at position 0 on the sentence's period is the clause
/// end of an empty prefix, and a supplied preposed duration wins.
#[test]
fn a_clause_end_at_position_zero_is_the_indefinite_duration() {
    let tokens = recognize_entities(".");
    let mut cursor = OracleTokenCursor::new("duration-position-zero", &tokens);
    assert_eq!(
        super::recognize_trailing_or_supplied_duration(&mut cursor, None),
        Ok(EffectDurationNode::Indefinite)
    );
    let mut cursor = OracleTokenCursor::new("duration-position-zero", &tokens);
    assert_eq!(
        super::recognize_trailing_or_supplied_duration(
            &mut cursor,
            Some(&EffectDurationNode::UntilEndOfCurrentTurn)
        ),
        Ok(EffectDurationNode::UntilEndOfCurrentTurn)
    );
}

/// `for as long as STATE` is an effect duration over the source's watched
/// state (CR 611.2b); `this artifact` is the current-object reference.
#[test]
fn source_state_windows_are_effect_durations() {
    for (text, expected) in [
        (
            "for as long as this artifact remains tapped",
            EffectDurationNode::WhileSourceTapped,
        ),
        (
            "for as long as you control this creature",
            EffectDurationNode::WhileSourceControlled,
        ),
        (
            "for as long as this creature remains on the battlefield",
            EffectDurationNode::WhileSourceOnBattlefield,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("duration-test", &tokens);
        assert_eq!(
            recognize_effect_duration(&mut cursor),
            Ok(expected),
            "{text}"
        );
        assert_eq!(cursor.position(), tokens.len(), "{text}");
    }
}

/// Near misses: the state of another object is not a source window, and
/// `until your next upkeep` is neither `until end of turn` nor `until your
/// next turn`.
#[test]
fn other_watched_objects_and_upkeep_windows_fail_with_their_own_state() {
    let tokens = recognize_entities("for as long as that creature has a bounty counter on it");
    let mut cursor = OracleTokenCursor::new("duration-test", &tokens);
    let failure = recognize_effect_duration(&mut cursor).expect_err("another object's state");
    assert_eq!(failure.state, "duration.while-condition.subject");

    let tokens = recognize_entities("until your next upkeep");
    let mut cursor = OracleTokenCursor::new("duration-test", &tokens);
    let failure = recognize_effect_duration(&mut cursor).expect_err("an upkeep window");
    assert_eq!(failure.state, "duration.until-end-of-turn.unit");
}

/// A preposed duration excludes a second trailing `for as long as`.
#[test]
fn a_supplied_duration_rejects_a_trailing_watched_window() {
    let tokens = recognize_entities("for as long as this artifact remains tapped");
    let mut cursor = OracleTokenCursor::new("duration-test", &tokens);
    assert!(
        super::recognize_trailing_or_supplied_duration(
            &mut cursor,
            Some(&EffectDurationNode::UntilEndOfCurrentTurn)
        )
        .is_err()
    );
}
