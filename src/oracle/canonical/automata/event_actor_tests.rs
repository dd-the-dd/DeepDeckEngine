use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

#[test]
fn shared_actor_scope_leaves_the_event_verb_to_the_parent() {
    for (text, expected, verb) in [
        ("you cast", EventActorScope::Controller, "cast"),
        (
            "a player sacrifices",
            EventActorScope::AnyPlayer,
            "sacrifices",
        ),
        ("an opponent casts", EventActorScope::Opponent, "casts"),
    ] {
        let tokens = recognize_oracle_entities(text);
        let mut cursor = OracleTokenCursor::new("event-actor-test", &tokens);
        assert_eq!(
            try_recognize_event_actor_scope(&mut cursor).unwrap(),
            Some(expected)
        );
        assert_eq!(cursor.peek(), Some(verb));
    }
}

#[test]
fn nonplayer_subject_restores_a_nonzero_checkpoint() {
    let tokens = recognize_oracle_entities("prefix a creature enters");
    let mut cursor = OracleTokenCursor::new("event-actor-test", &tokens);
    cursor.consume("prefix").unwrap();
    let position = cursor.position();
    assert_eq!(try_recognize_event_actor_scope(&mut cursor).unwrap(), None);
    assert_eq!(cursor.position(), position);
    assert_eq!(cursor.peek(), Some("a"));
}
