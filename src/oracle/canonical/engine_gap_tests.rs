use super::*;

#[test]
fn every_engine_gap_has_a_distinct_reserved_state() {
    let mut seen = std::collections::HashSet::new();
    for gap in EngineGap::ALL {
        let state = gap.state();
        assert!(is_engine_gap_state(state), "{state}");
        assert!(seen.insert(state), "duplicate engine gap state {state}");
        assert_eq!(EngineGap::from_state(state), Some(gap));
    }
}

#[test]
fn grammar_states_are_never_engine_gaps() {
    for state in [
        "effect.start",
        "trigger.event.engine-gap",
        "lower.program.optional-action.action",
        "keyword.unsupported",
    ] {
        assert!(!is_engine_gap_state(state), "{state}");
        assert_eq!(EngineGap::from_state(state), None, "{state}");
    }
}
