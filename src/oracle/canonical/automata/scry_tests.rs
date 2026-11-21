use crate::oracle::syntax::recognize_entities;

use super::super::{
    NumericAtom, NumericExpression, PlayerPredicate, TargetBinding, TargetSelector,
};
use super::{
    OracleTokenCursor, recognize_scry_effect, recognize_scry_reminder,
    recognize_target_player_scry_then_draw,
};

#[test]
fn scry_reuses_literal_and_variable_numeric_expressions() {
    for (text, expected) in [
        ("Scry 2.", NumericExpression::Atom(NumericAtom::Integer(2))),
        ("Scry X.", NumericExpression::Atom(NumericAtom::VariableX)),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.scry", &tokens);
        let effect = recognize_scry_effect(&mut cursor).expect("scry effect parses");
        assert_eq!(effect.count, expected);
        assert_eq!(cursor.peek(), Some("."));
    }
}

#[test]
fn singular_reminder_is_validated_token_by_token() {
    let tokens = recognize_entities(
        "Scry 1. (Look at the top card of your library. You may put that card on the bottom.)",
    );
    let mut cursor = OracleTokenCursor::new("test.scry", &tokens);
    let effect = recognize_scry_effect(&mut cursor).expect("scry effect parses");
    cursor.consume(".").expect("effect period");
    recognize_scry_reminder(&mut cursor, &effect.count).expect("canonical reminder parses");
    cursor.finish().expect("full reminder consumed");
}

#[test]
fn reminder_near_miss_reports_the_precise_destination_state() {
    let tokens = recognize_entities(
        "Scry 1. (Look at the top card of your library. You may put that card in your graveyard.)",
    );
    let mut cursor = OracleTokenCursor::new("test.scry", &tokens);
    let effect = recognize_scry_effect(&mut cursor).expect("scry effect parses");
    cursor.consume(".").expect("effect period");
    let failure = recognize_scry_reminder(&mut cursor, &effect.count)
        .expect_err("a surveil-like destination must not become scry");
    assert_eq!(failure.state, "effect.scry.reminder.destination");
    assert_eq!(failure.actual.as_deref(), Some("in"));
    assert_eq!(failure.expected, "on");
}

#[test]
fn target_player_scry_then_draw_keeps_two_ordered_primitive_effects() {
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    for (text, predicate, count) in [
        (
            "Target player scries X, then draws a card.",
            PlayerPredicate::Any,
            NumericAtom::VariableX,
        ),
        (
            "Target opponent scries 2, then draws a card.",
            PlayerPredicate::Opponent,
            NumericAtom::Integer(2),
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.target-player-scry-draw", &tokens);
        let sequence = recognize_target_player_scry_then_draw(&mut cursor, binding)
            .expect("target-player scry/draw sequence parses");

        assert_eq!(sequence.target.slots.len(), 1);
        assert_eq!(
            sequence.target.slots[0].selector,
            TargetSelector::Player(predicate)
        );
        assert_eq!(sequence.target_binding, binding);
        assert_eq!(sequence.scry_count, NumericExpression::Atom(count));
        assert_eq!(
            sequence.draw_count,
            NumericExpression::Atom(NumericAtom::Integer(1))
        );
        assert_eq!(cursor.peek(), Some("."));
    }
}

#[test]
fn target_player_scry_then_draw_rejects_nonsequential_connector() {
    let tokens = recognize_entities("Target player scries X, and draws a card.");
    let mut cursor = OracleTokenCursor::new("test.target-player-scry-draw", &tokens);
    let failure = recognize_target_player_scry_then_draw(
        &mut cursor,
        TargetBinding {
            effect_index: 0,
            slot_index: 0,
        },
    )
    .expect_err("`and` must not silently become ordered `then`");

    assert_eq!(failure.state, "effect.target-player-scry-draw.connector");
    assert_eq!(failure.actual.as_deref(), Some("and"));
    assert_eq!(failure.expected, "then");
}
