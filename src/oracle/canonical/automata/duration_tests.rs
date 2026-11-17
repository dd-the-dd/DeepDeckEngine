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
