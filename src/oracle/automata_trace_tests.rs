use super::*;
use serde_json::json;

fn request(text: &str) -> OracleCardParseRequest {
    serde_json::from_value(json!({
        "cardName":"Trace Probe","typeLine":"Sorcery","manaCost":"{W}",
        "oracleText":text,"layout":"normal","faces":[],
    }))
    .unwrap()
}

#[test]
fn traced_cards_preserve_complete_results_and_isolate_ability_scopes() {
    for text in [
        "Draw a card.\nYou gain 1 life.",
        "Invent an unknown action.",
        "",
    ] {
        let input = request(text);
        let ordinary = super::super::parse_oracle_card_native(input.clone());
        let traced = parse_oracle_card_native_traced(input.clone());
        assert_eq!(
            serde_json::to_value(&ordinary).unwrap(),
            serde_json::to_value(&traced.result).unwrap()
        );
        assert!(!traced.trace.truncated);
        assert!(traced.trace.events.iter().all(|event| {
            event
                .ability_index
                .is_some_and(|index| index < traced.result.abilities.len())
        }));
        if text.starts_with("Draw") {
            assert!(
                traced
                    .trace
                    .events
                    .iter()
                    .any(|event| event.ability_index == Some(0))
            );
            assert!(
                traced
                    .trace
                    .events
                    .iter()
                    .any(|event| event.ability_index == Some(1))
            );
        }
        let again = parse_oracle_card_native_traced(input);
        assert_eq!(
            serde_json::to_value(&traced.trace).unwrap(),
            serde_json::to_value(&again.trace).unwrap(),
            "a card never inherits a previous card's trace",
        );
        assert!(ACTIVE.with(|active| active.borrow().is_none()));
    }
}

#[test]
fn sessions_are_thread_local_without_cross_card_events() {
    let jobs: Vec<_> = ["Draw a card.", "You gain 1 life."]
        .into_iter()
        .map(|text| std::thread::spawn(move || parse_oracle_card_native_traced(request(text))))
        .collect();
    for job in jobs {
        let result = job.join().unwrap();
        assert!(!result.trace.events.is_empty());
        assert_eq!(result.trace.events[0].cursor_id, 1);
        assert!(
            result
                .trace
                .events
                .iter()
                .all(|event| event.ability_index == Some(0))
        );
        assert_eq!(
            result
                .result
                .context
                .oracle_text
                .as_deref()
                .unwrap()
                .lines()
                .count(),
            1
        );
    }
    assert!(ACTIVE.with(|active| active.borrow().is_none()));
}

#[test]
fn nested_and_unwinding_sessions_restore_the_outer_collector() {
    let (_, trace) = collect_trace(|| {
        let _ability = scope_ability(7);
        let handle = new_cursor(
            None,
            "outer",
            "start",
            0,
            None,
            None,
            AutomataTraceEventKind::New,
        );
        let (_, inner) = collect_trace(|| {
            let _ability = scope_ability(2);
            new_cursor(
                None,
                "inner",
                "start",
                0,
                None,
                None,
                AutomataTraceEventKind::New,
            );
            // An old handle cannot inject an event into the inner card.
            record(
                handle,
                "outer",
                "foreign",
                0,
                None,
                None,
                AutomataTraceEventKind::Enter,
            );
        });
        assert_eq!(inner.events.len(), 1);
        assert_eq!(inner.events[0].ability_index, Some(2));
        let error = std::panic::catch_unwind(|| collect_trace(|| panic!("trace cleanup probe")));
        assert!(error.is_err());
        record(
            handle,
            "outer",
            "resumed",
            1,
            None,
            None,
            AutomataTraceEventKind::Enter,
        );
    });
    assert_eq!(trace.events.len(), 2);
    assert!(
        trace
            .events
            .iter()
            .all(|event| event.automaton == "outer" && event.ability_index == Some(7))
    );
    assert_eq!(trace.events[1].state, "resumed");
    assert!(ACTIVE.with(|active| active.borrow().is_none()));
}

#[test]
fn event_limit_is_bounded_and_explicitly_marks_the_missing_tail() {
    let (_, trace) = collect_trace(|| {
        let handle = new_cursor(
            None,
            "limit",
            "start",
            0,
            None,
            None,
            AutomataTraceEventKind::New,
        );
        for index in 0..=EVENT_LIMIT {
            record(
                handle,
                "limit",
                "read",
                index,
                None,
                None,
                AutomataTraceEventKind::Enter,
            );
        }
    });
    assert_eq!(trace.events.len(), EVENT_LIMIT);
    assert!(trace.truncated);
}
