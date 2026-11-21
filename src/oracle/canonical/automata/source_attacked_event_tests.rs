use crate::oracle::syntax::recognize_entities;

use super::{
    LocalObjectReference, LocalTriggerEventNode, OracleTokenCursor, recognize_source_attacked_event,
};

#[test]
fn typed_this_source_attacked_event_is_recognized_without_owning_the_envelope() {
    let tokens = recognize_entities("this land attacks,");
    let mut cursor = OracleTokenCursor::new("test.source-attacked-event", &tokens);

    let event = recognize_source_attacked_event(&mut cursor, &[], None)
        .expect("source-attacked event parses");

    assert_eq!(
        event,
        LocalTriggerEventNode::SourceAttacked {
            source: LocalObjectReference::SourceSelf,
        }
    );
    assert_eq!(cursor.peek(), Some(","));
}

#[test]
fn source_attack_event_rejects_a_different_event_word() {
    let tokens = recognize_entities("this land enters,");
    let mut cursor = OracleTokenCursor::new("test.invalid-source-attacked-event", &tokens);

    let failure = recognize_source_attacked_event(&mut cursor, &[], None)
        .expect_err("entered event is not an attacked event");

    assert!(failure.expected.contains("attacks"));
}
