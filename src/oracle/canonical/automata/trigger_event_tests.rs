use crate::oracle::syntax::recognize_entities;

use super::super::{
    DrawEventException, DrawPeriod, LocalObjectReference, LocalPlayerReference,
    LocalTriggerEventNode, PlayerPredicate,
};
use super::{
    OracleTokenCursor, recognize_entry_and_repeat_event_alternatives,
    recognize_ordinal_card_drawn_event,
};

#[test]
fn arbitrary_ordinal_draw_in_a_turn_is_a_typed_event() {
    let tokens = recognize_entities("you draw your fourth card in a turn");
    let mut cursor = OracleTokenCursor::new("ordinal-draw-test", &tokens);

    let event = recognize_ordinal_card_drawn_event(&mut cursor).expect("event parses");

    assert_eq!(
        event,
        LocalTriggerEventNode::OrdinalCardDrawn {
            player: LocalPlayerReference::Controller,
            ordinal: 4,
            period: DrawPeriod::Turn,
        }
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn ordinal_draw_does_not_accept_a_different_counted_action() {
    let tokens = recognize_entities("you cast your third spell in a turn");
    let mut cursor = OracleTokenCursor::new("ordinal-draw-test", &tokens);

    let failure = recognize_ordinal_card_drawn_event(&mut cursor)
        .expect_err("spell casting must not become card drawing");

    assert_eq!(failure.actual.as_deref(), Some("cast"));
    assert_eq!(failure.expected, "draw");
}

#[test]
fn entry_and_repeated_draw_are_typed_alternative_events() {
    let tokens = recognize_entities(
        "When this creature enters and whenever an opponent draws a card except the first one they draw in each of their draw steps,",
    );
    let mut cursor = OracleTokenCursor::new("event-alternatives-test", &tokens);

    let alternatives = recognize_entry_and_repeat_event_alternatives(&mut cursor, &[], None)
        .expect("alternative events parse");

    assert_eq!(
        alternatives.events,
        [
            LocalTriggerEventNode::SourceEntered {
                source: LocalObjectReference::SourceSelf,
            },
            LocalTriggerEventNode::CardDrawn {
                player: PlayerPredicate::Opponent,
                exception: Some(DrawEventException::FirstInEachDrawStep),
            },
        ]
    );
}

#[test]
fn source_type_and_player_role_vary_without_changing_event_structure() {
    let tokens =
        recognize_entities("When this planeswalker enters and whenever a player draws a card,");
    let mut cursor = OracleTokenCursor::new("event-alternatives-test", &tokens);

    let alternatives = recognize_entry_and_repeat_event_alternatives(&mut cursor, &[], None)
        .expect("registry-backed source and player parse");

    assert!(matches!(
        alternatives.events.as_slice(),
        [
            LocalTriggerEventNode::SourceEntered { .. },
            LocalTriggerEventNode::CardDrawn {
                player: PlayerPredicate::Any,
                exception: None,
            },
        ]
    ));
}

#[test]
fn unsupported_source_type_fails_in_the_shared_reference_machine() {
    let tokens =
        recognize_entities("When this tribal enters and whenever an opponent draws a card,");
    let mut cursor = OracleTokenCursor::new("event-alternatives-test", &tokens);

    let failure = recognize_entry_and_repeat_event_alternatives(&mut cursor, &[], None)
        .expect_err("unknown source type must not be guessed");

    assert_eq!(failure.actual.as_deref(), Some("tribal"));
    assert!(failure.expected.contains("source-object type"));
}

#[test]
fn draw_exception_rejects_a_different_ordinal_at_its_own_span() {
    let tokens = recognize_entities(
        "When this creature enters and whenever an opponent draws a card except the second one they draw in each of their draw steps,",
    );
    let mut cursor = OracleTokenCursor::new("event-alternatives-test", &tokens);

    let failure = recognize_entry_and_repeat_event_alternatives(&mut cursor, &[], None)
        .expect_err("only the declared first-draw exception is supported");

    assert_eq!(failure.actual.as_deref(), Some("second"));
    assert_eq!(failure.expected, "the first draw ordinal");
}
