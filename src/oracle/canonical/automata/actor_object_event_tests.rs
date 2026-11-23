use super::*;
use crate::oracle::syntax::recognize_entities;

/// The event clause owns the trigger word and separator; the clause child
/// starts after the trigger word and stops before the separator.
fn clause(text: &str) -> (ActorObjectEventAttempt, usize, Option<String>) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.event-alternatives", &tokens);
    cursor.consume("whenever").expect("trigger word");
    let attempt = try_recognize_actor_object_event_clause(&mut cursor);
    (
        attempt,
        cursor.position(),
        cursor.peek().map(ToOwned::to_owned),
    )
}

#[test]
fn alternatives_share_actor_and_subject_and_preserve_branch_order() {
    for (text, count) in [
        (
            "Whenever you create or sacrifice a token, each opponent loses 1 life.",
            2,
        ),
        (
            "Whenever you sacrifice, create, or sacrifice a token, you gain 1 life.",
            3,
        ),
    ] {
        let (ActorObjectEventAttempt::Recognized(event), _, next) = clause(text) else {
            panic!("typed event");
        };
        assert_eq!(event.actor, EventActorScope::Controller);
        assert_eq!(event.actions.len(), count);
        assert_eq!(event.object.predicate, ObjectPredicate::IsToken);
        assert_eq!(next.as_deref(), Some(","), "{text}");
    }
}

#[test]
fn single_sacrifice_keeps_its_reader_and_conjunctive_or_quantity_events_do_not_widen() {
    let (attempt, position, _) = clause("Whenever you sacrifice a token, draw a card.");
    assert!(matches!(attempt, ActorObjectEventAttempt::NotClaimed));
    assert_eq!(position, 1, "rolled back to the clause start");
    for text in [
        "Whenever you create and sacrifice a token, draw a card.",
        "Whenever you create one or more tokens, draw a card.",
        "Whenever you create or sacrifice a token an opponent controls, draw a card.",
    ] {
        let (attempt, _, _) = clause(text);
        assert!(
            matches!(attempt, ActorObjectEventAttempt::Rejected(_)),
            "{text}"
        );
    }
}
