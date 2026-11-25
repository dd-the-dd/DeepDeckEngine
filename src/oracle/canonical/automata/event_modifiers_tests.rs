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
    // `during combat` is a combat window, not a turn owner (CR 506.1).
    let (result, next) = tail("during combat, draw a card.");
    let modifiers = result.expect("combat window");
    assert!(modifiers.during_combat && modifiers.turn_owner.is_none());
    assert_eq!(next.as_deref(), Some(","));
    // `during the declare attackers step` names no typed window: the tail
    // leaves it to the clause, which fails at its boundary.
    let (result, next) = tail("during the declare attackers step, draw a card.");
    assert!(result.expect("empty tail").is_empty());
    assert_eq!(next.as_deref(), Some("during"));
}

#[test]
fn a_repeated_modifier_is_rejected() {
    let (result, _) = tail("during your turn during your turn, draw a card.");
    let failure = result.expect_err("duplicate turn owner");
    assert_eq!(failure.state, "event.modifier.turn-owner");
}

#[test]
fn distributive_turn_owners_name_the_same_relation_for_every_turn() {
    for (text, owner, distributive) in [
        (
            "during each opponent's turn, draw a card.",
            EventTurnOwner::OpponentTurn,
            true,
        ),
        (
            "during each of your turns, draw a card.",
            EventTurnOwner::ControllerTurn,
            true,
        ),
        (
            "during an opponent's turn, draw a card.",
            EventTurnOwner::OpponentTurn,
            false,
        ),
        (
            "during your turn, draw a card.",
            EventTurnOwner::ControllerTurn,
            false,
        ),
    ] {
        let tokens = recognize_oracle_entities(text);
        let mut cursor = OracleTokenCursor::new("turn-owner-test", &tokens);
        assert!(turn_owner_modifier_starts(&cursor), "{text}");
        let read = recognize_turn_owner_period(&mut cursor).expect(text);
        assert_eq!(read, (owner, distributive), "{text}");
        assert_eq!(cursor.peek(), Some(","), "{text}");
    }
    // `each player's turn` names no turn owner relation.
    let tokens = recognize_oracle_entities("during each player's turn, draw a card.");
    let cursor = OracleTokenCursor::new("turn-owner-test", &tokens);
    assert!(!turn_owner_modifier_starts(&cursor));
}

#[test]
fn the_shared_tail_leaves_event_slot_qualifiers_to_their_frame() {
    for text in [
        "without dying, draw a card.",
        "other than the battlefield, draw a card.",
    ] {
        let (result, next) = tail(text);
        assert!(result.expect("empty tail").is_empty(), "{text}");
        assert_ne!(next.as_deref(), Some(","), "{text}");
    }
}
