use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn read(text: &str) -> MachineResult<CounterPlacementEventNode> {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("counter-event-test", &tokens);
    recognize_counter_placement_event(&mut cursor, &[], None)
}

#[test]
fn counter_kind_recipient_and_placer_are_independent_attributes() {
    let source = read("one or more +1/+1 counters are put on this creature").unwrap();
    assert_eq!(source.placed_by, None);
    assert_eq!(
        source.counter.as_ref().map(CounterName::as_str),
        Some("+1/+1")
    );
    assert_eq!(
        source.recipient,
        CounterRecipient::Source(LocalObjectReference::SourceSelf)
    );

    let other = read("one or more -1/-1 counters are put on another creature you control").unwrap();
    assert_eq!(
        other.counter.as_ref().map(CounterName::as_str),
        Some("-1/-1")
    );
    assert!(matches!(
        other.recipient,
        CounterRecipient::Query { exclude_source: true, ref query }
            if query.controller.is_some()
    ));

    let placed = read("you put one or more +1/+1 counters on a creature").unwrap();
    assert_eq!(placed.placed_by, Some(EventActorScope::Controller));

    let any_kind = read("one or more counters are put on this creature").unwrap();
    assert_eq!(any_kind.counter, None);
}

#[test]
fn counter_event_near_misses_keep_their_grammar_state() {
    // The singular `a counter is put on` counts counters, not placements.
    assert_eq!(
        read("a +1/+1 counter is put on this creature")
            .unwrap_err()
            .state,
        "event.counters.batch"
    );
    // The placer agrees with the verb form.
    assert_eq!(
        read("you puts one or more +1/+1 counters on a creature")
            .unwrap_err()
            .state,
        "event.counters.action"
    );
    // A player is not a permanent query: the shared selector rejects it.
    let failure = read("one or more +1/+1 counters are put on a player").unwrap_err();
    assert_eq!(failure.state, "selector.predicate.start", "{failure}");
}
