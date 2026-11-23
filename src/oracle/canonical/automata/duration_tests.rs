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
