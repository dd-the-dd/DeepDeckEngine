use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn tail(text: &str) -> (MachineResult<EventModifierTail>, Option<String>) {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("event-modifier-test", &tokens);
    let result = read_event_modifier_tail(&mut cursor);
    (result, cursor.peek().map(ToOwned::to_owned))
}

#[test]
fn modifiers_are_typed_in_any_written_order_and_stop_at_the_boundary() {
    let (result, next) = tail("and isn't blocked for the first time each turn, draw a card.");
    let modifiers = result.expect("modifier tail");
    assert!(modifiers.not_blocked && modifiers.first_time_each_turn);
    assert_eq!(modifiers.turn_owner, None);
    assert_eq!(next.as_deref(), Some(","));

    let (result, next) = tail("during an opponent's turn, draw a card.");
    assert_eq!(
        result.expect("turn owner").turn_owner,
        Some(EventTurnOwner::OpponentTurn)
    );
    assert_eq!(next.as_deref(), Some(","));

    let (result, _) = tail("during your turn, draw a card.");
    assert_eq!(
        result.expect("turn owner").turn_owner,
        Some(EventTurnOwner::ControllerTurn)
    );
}

#[test]
fn an_unmodified_clause_reads_nothing() {
    let (result, next) = tail(", draw a card.");
    assert!(result.expect("empty tail").is_empty());
    assert_eq!(next.as_deref(), Some(","));
    // `during combat` is not a turn owner: the clause keeps the tokens.
    let (result, next) = tail("during combat, draw a card.");
    assert!(result.expect("empty tail").is_empty());
    assert_eq!(next.as_deref(), Some("during"));
}

#[test]
fn a_repeated_modifier_is_rejected() {
    let (result, _) = tail("during your turn during your turn, draw a card.");
    let failure = result.expect_err("duplicate turn owner");
    assert_eq!(failure.state, "event.modifier.turn-owner");
}
