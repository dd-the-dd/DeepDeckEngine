//! The event count reads its type filter and player population, and refuses
//! a filter it cannot read (`countEventsThisTurn`).
use super::*;

fn died(engine: &mut GameEngine, id: &str, type_line: &str, player: &str) {
    engine.record_event(
        "permanentDied",
        Some(player.to_string()),
        Some(id.to_string()),
        json!({ "typeLine": type_line, "wasCreature": type_line.contains("Creature") }),
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_event_count_reads_its_type_filter_and_its_player_population() {
    let mut engine = test_engine(2);
    died(&mut engine, "bear", "Creature \u{2014} Bear", "player-0");
    died(&mut engine, "relic", "Artifact", "player-1");
    died(
        &mut engine,
        "golem",
        "Artifact Creature \u{2014} Golem",
        "player-1",
    );
    let creatures = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    });
    assert_eq!(engine.count_matching_events(&creatures, None), Some(2));
    // Sibling: a negated type atom and an opponent population.
    let noncreature = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "where": { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Creature" } },
    });
    assert_eq!(engine.count_matching_events(&noncreature, None), Some(1));
    let opponents = vec!["player-1".to_string()];
    assert_eq!(
        engine.count_matching_events(&creatures, Some(&opponents)),
        Some(1)
    );
    // `Creature` is a word of the type line, not a substring of a subtype.
    let subtype = json!({
        "kind": "countEventsThisTurn",
        "event": "permanentDied",
        "where": { "kind": "subtypeContains", "value": "Bea" },
    });
    assert_eq!(engine.count_matching_events(&subtype, None), Some(0));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_event_count_refuses_a_filter_it_cannot_read() {
    // Near misses: a non-type atom, a type filter on an event that records
    // no type line, and a previous-turn count with a filter.
    for expression in [
        json!({
            "kind": "countEventsThisTurn",
            "event": "permanentDied",
            "where": { "kind": "isToken" },
        }),
        json!({
            "kind": "countEventsThisTurn",
            "event": "spellCast",
            "where": { "kind": "cardTypeContains", "value": "Instant" },
        }),
    ] {
        assert!(
            !event_count::event_count_supported(&expression),
            "{expression}"
        );
        assert!(!numeric_expression_supported(&expression), "{expression}");
    }
    assert!(!numeric_expression_supported(&json!({
        "kind": "countEventsPreviousTurn",
        "event": "spellCast",
        "aggregate": "total",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    })));
    assert!(event_count::event_count_supported(&json!({
        "kind": "countEventsThisTurn",
        "event": "spellCast",
        "player": { "kind": "opponentsOf", "player": { "kind": "abilityController" } },
        "where": null,
    })));
}
