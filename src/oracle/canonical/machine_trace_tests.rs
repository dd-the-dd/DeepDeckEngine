use super::*;
use crate::oracle::automata_trace::{collect_trace, scope_ability};

fn tokens() -> Vec<RecognizedEntity> {
    vec![RecognizedEntity {
        category: "token".to_string(),
        index: 0,
        raw: "é".to_string(),
        span_start: 0,
        span_end: 2,
        token_type: "word".to_string(),
    }]
}

#[test]
fn cloned_cursors_have_distinct_identity_and_only_true_clone_parentage() {
    let tokens = tokens();
    let (_, trace) = collect_trace(|| {
        let _ability = scope_ability(0);
        let mut parent = OracleTokenCursor::new("fork", &tokens);
        let mut child = parent.clone();
        child.enter("child.branch");
        parent.enter("parent.branch");
        let unrelated = OracleTokenCursor::new("independent", &tokens);
        assert_eq!(unrelated.position(), 0);
    });
    let parent = &trace.events[0];
    let clone = &trace.events[1];
    assert_eq!(parent.kind, AutomataTraceEventKind::New);
    assert_eq!(clone.kind, AutomataTraceEventKind::Clone);
    assert_ne!(parent.cursor_id, clone.cursor_id);
    assert_eq!(clone.parent_cursor_id, Some(parent.cursor_id));
    assert_eq!(trace.events[2].cursor_id, clone.cursor_id);
    assert_eq!(trace.events[3].cursor_id, parent.cursor_id);
    assert!(trace.events[4].parent_cursor_id.is_none());
}

#[test]
fn real_failure_rollback_and_finish_keep_parser_semantics_and_utf8_spans() {
    let tokens = tokens();
    let (failure, trace) = collect_trace(|| {
        let mut cursor = OracleTokenCursor::new("transaction", &tokens);
        let failure = cursor
            .attempt(|trial| {
                trial.enter("attempt.read");
                trial.consume("é")?;
                trial.consume("missing")
            })
            .unwrap_err();
        assert_eq!(cursor.position(), 0);
        assert_eq!(cursor.peek(), Some("é"));
        cursor.consume_type("word").unwrap();
        cursor.finish().unwrap();
        failure
    });
    assert_eq!(failure.token_index, 1);
    assert_eq!(failure.span_start, 2);
    assert_eq!(failure.span_end, 2);
    let kinds: Vec<_> = trace.events.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        [
            AutomataTraceEventKind::New,
            AutomataTraceEventKind::Checkpoint,
            AutomataTraceEventKind::Enter,
            AutomataTraceEventKind::Consume,
            AutomataTraceEventKind::Failure,
            AutomataTraceEventKind::Rollback,
            AutomataTraceEventKind::Consume,
            AutomataTraceEventKind::Finish,
        ]
    );
    assert_eq!(trace.events[3].span_end, Some(2));
    assert_eq!(trace.events[3].token_text.as_deref(), Some("é"));
    assert_eq!(trace.events[5].token_index, 0);
    assert_eq!(trace.events[5].state, "start");
    assert_eq!(trace.events.last().unwrap().token_index, 1);
}

#[test]
fn advance_and_explicit_failure_spans_are_observed_without_new_transitions() {
    let tokens = tokens();
    let (_, trace) = collect_trace(|| {
        let mut cursor = OracleTokenCursor::new("advance", &tokens);
        cursor.advance_to(1).unwrap();
        let failure = cursor.fail_with_span("a word", 0, 0, 2, "é");
        assert_eq!(failure.trace.as_slice(), cursor.transitions());
        assert_eq!(cursor.transitions().len(), 1);
    });
    assert_eq!(trace.events[1].kind, AutomataTraceEventKind::Advance);
    assert_eq!(trace.events[1].token_index, 1);
    assert_eq!(trace.events[2].kind, AutomataTraceEventKind::Failure);
    assert_eq!(trace.events[2].token_index, 0);
    assert_eq!(trace.events[2].span_start, Some(0));
    assert_eq!(trace.events[2].span_end, Some(2));
}
