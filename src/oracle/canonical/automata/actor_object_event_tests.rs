use super::*;
use crate::oracle::syntax::recognize_entities;

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
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.event-alternatives", &tokens);
        let ActorObjectEventAttempt::Recognized(event) =
            try_recognize_actor_object_event(&mut cursor)
        else {
            panic!("typed event");
        };
        assert_eq!(event.actor, EventActorScope::Controller);
        assert_eq!(event.actions.len(), count);
        assert_eq!(event.object.predicate, ObjectPredicate::IsToken);
        assert_eq!(cursor.peek(), Some(if count == 2 { "each" } else { "you" }));
    }
}

#[test]
fn single_sacrifice_keeps_its_reader_and_conjunctive_or_quantity_events_do_not_widen() {
    let tokens = recognize_entities("Whenever you sacrifice a token, draw a card.");
    let mut cursor = OracleTokenCursor::new("test.event-alternatives", &tokens);
    assert!(matches!(
        try_recognize_actor_object_event(&mut cursor),
        ActorObjectEventAttempt::NotClaimed
    ));
    assert_eq!(cursor.position(), 0);
    for text in [
        "Whenever you create and sacrifice a token, draw a card.",
        "Whenever you create one or more tokens, draw a card.",
        "Whenever you create or sacrifice a token an opponent controls, draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.event-alternatives", &tokens);
        assert!(matches!(
            try_recognize_actor_object_event(&mut cursor),
            ActorObjectEventAttempt::Rejected(_)
        ));
    }
}
