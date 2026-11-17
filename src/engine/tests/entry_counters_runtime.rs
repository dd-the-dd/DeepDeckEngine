use super::*;

/// CR 614.1c: the counters are part of how the permanent enters, so they are
/// on the object before it is committed to the battlefield.
fn entering_with(count: Value, counter: &str) -> CardInstance {
    let mut definition = test_definition("entrant", "Creature - Test");
    definition.rules = vec![json!({
        "kind": "replacementEffect",
        "source": { "kind": "self" },
        "event": { "kind": "wouldEnterBattlefield", "object": { "kind": "self" } },
        "replacement": [{ "kind": "putEnteringCounters", "counter": counter, "count": count }],
    })];
    test_instance("entrant", definition, "player-0")
}

fn enter(entering: &mut CardInstance, engine: &mut GameEngine, decisions: BTreeMap<String, Value>) {
    engine
        .apply_enter_replacements_with_decisions(
            entering,
            &mut EmeritusDecisionProvider,
            &decisions,
        )
        .expect("the entry-counter replacement resolves");
}

#[test]
fn literal_entry_counters_are_present_before_the_permanent_enters() {
    let mut engine = test_engine(2);
    let mut entering = entering_with(json!({"kind": "integer", "value": 2}), "-1/-1");
    enter(&mut entering, &mut engine, BTreeMap::new());
    assert_eq!(entering.counters.get("-1/-1"), Some(&2));
    assert!(engine.state.players[0].battlefield.is_empty());
}

#[test]
fn variable_entry_counters_read_the_declared_x() {
    let mut engine = test_engine(2);
    let mut entering = entering_with(
        json!({"kind": "decisionResult", "decisionId": "xValue"}),
        "+1/+1",
    );
    enter(
        &mut entering,
        &mut engine,
        BTreeMap::from([("xValue".to_string(), json!(3))]),
    );
    assert_eq!(entering.counters.get("+1/+1"), Some(&3));
}

#[test]
fn for_each_entry_counters_count_permanents_already_on_the_battlefield() {
    let mut engine = test_engine(2);
    for index in 0..2 {
        let id = format!("ally-{index}");
        let definition = test_definition(&id, "Creature - Ally");
        engine.state.players[0]
            .battlefield
            .push(test_instance(&id, definition, "player-0"));
    }
    let count = json!({
        "kind": "countPermanents",
        "where": { "kind": "cardTypeContains", "value": "Creature" },
        "player": { "kind": "abilityController" },
        "excludeSource": true,
    });
    let mut entering = entering_with(count, "+1/+1");
    enter(&mut entering, &mut engine, BTreeMap::new());
    assert_eq!(entering.counters.get("+1/+1"), Some(&2));
}
