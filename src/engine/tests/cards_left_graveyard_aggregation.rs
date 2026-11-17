use super::*;

fn graveyard_leave_rule(aggregation: Option<&str>) -> Value {
    let mut event = json!({
        "kind": "cardsLeftGraveyard",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "where": {
            "kind": "or",
            "operands": [
                { "kind": "cardTypeContains", "value": "Instant" },
                { "kind": "cardTypeContains", "value": "Sorcery" },
            ],
        },
    });
    if let Some(aggregation) = aggregation {
        event["aggregation"] = Value::String(aggregation.to_string());
    }
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": event,
        "effects": [{
            "kind": "putCounters",
            "permanent": { "kind": "self" },
            "counter": "+1/+1",
            "count": { "kind": "integer", "value": 1 },
        }],
    })
}

fn watcher(aggregation: Option<&str>) -> CardInstance {
    let mut definition = test_definition("graveyard-watcher", "Creature - Dragon");
    definition.power = Some("3".to_string());
    definition.toughness = Some("3".to_string());
    definition.rules = vec![graveyard_leave_rule(aggregation)];
    test_instance("graveyard-watcher", definition, "player-0")
}

fn moved_card(id: &str, type_line: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), "player-0")
}

#[test]
fn per_card_graveyard_leave_enqueues_and_resolves_one_trigger_per_matching_card() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![watcher(Some("perCard"))];
    engine.state.players[0].exile = vec![
        moved_card("moved-instant", "Instant"),
        moved_card("moved-sorcery", "Sorcery"),
        moved_card("moved-creature", "Creature"),
    ];

    engine.enqueue_cards_left_graveyard_triggers(
        "player-0",
        &[
            "moved-instant".to_string(),
            "moved-sorcery".to_string(),
            "moved-creature".to_string(),
        ],
    );

    assert_eq!(engine.state.stack.len(), 2);
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("first per-card trigger resolves");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("second per-card trigger resolves");
    let watcher = &engine.state.players[0].battlefield[0];
    assert_eq!(watcher.counters.get("+1/+1"), Some(&2));
}

#[test]
fn one_or_more_graveyard_leave_enqueues_one_trigger_for_the_matching_batch() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![watcher(Some("oneOrMore"))];
    engine.state.players[0].exile = vec![
        moved_card("moved-instant", "Instant"),
        moved_card("moved-sorcery", "Sorcery"),
    ];

    engine.enqueue_cards_left_graveyard_triggers(
        "player-0",
        &["moved-instant".to_string(), "moved-sorcery".to_string()],
    );

    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn missing_aggregation_preserves_the_historical_one_or_more_batch_semantics() {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![watcher(None)];
    engine.state.players[0].exile = vec![
        moved_card("moved-instant", "Instant"),
        moved_card("moved-sorcery", "Sorcery"),
    ];

    engine.enqueue_cards_left_graveyard_triggers(
        "player-0",
        &["moved-instant".to_string(), "moved-sorcery".to_string()],
    );

    assert_eq!(engine.state.stack.len(), 1);
}
