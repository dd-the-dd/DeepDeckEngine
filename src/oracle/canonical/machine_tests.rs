use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn cursor_borrows_one_token_stream_and_reports_the_precise_failed_span() {
    let tokens = recognize_entities("This spell deals 3 damage.");
    let mut cursor = OracleTokenCursor::new("test", &tokens);
    cursor.enter("source");
    cursor.consume_phrase(&["This", "spell"]).unwrap();
    cursor.enter("action");
    cursor.consume("deals").unwrap();
    cursor.enter("amount");
    cursor.consume_type("quantity").unwrap();
    cursor.consume("damage").unwrap();
    cursor.enter("end");
    let failure = cursor.finish().unwrap_err();

    assert_eq!(failure.automaton, "test");
    assert_eq!(failure.state, "end");
    assert_eq!(failure.actual.as_deref(), Some("."));
    assert_eq!((failure.span_start, failure.span_end), (25, 26));
    assert_eq!(failure.trace.last().unwrap().state, "end");
}

#[test]
fn failed_local_attempt_rolls_back_cursor_and_transient_state() {
    let tokens = recognize_entities("any target");
    let mut cursor = OracleTokenCursor::new("test", &tokens);
    let failure = cursor
        .attempt(|candidate| {
            candidate.enter("target-player");
            candidate.consume_phrase(&["target", "player"])
        })
        .unwrap_err();

    assert_eq!(failure.state, "target-player");
    assert_eq!(failure.token_index, 0);
    assert_eq!(cursor.position(), 0);
    assert_eq!(cursor.transitions().last().unwrap().state, "rollback");

    cursor.consume_phrase(&["any", "target"]).unwrap();
    assert!(cursor.finish().is_ok());
}

#[test]
fn end_of_input_failure_uses_the_empty_span_after_the_last_token() {
    let tokens = recognize_entities("gain");
    let mut cursor = OracleTokenCursor::new("test", &tokens);
    cursor.enter("amount");
    cursor.consume("gain").unwrap();
    let failure = cursor.consume_type("quantity").unwrap_err();

    assert_eq!(failure.token_index, 1);
    assert_eq!((failure.span_start, failure.span_end), (4, 4));
    assert_eq!(failure.actual, None);
}
