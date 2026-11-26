//! Real Oracle text -> primitive IR -> engine behavior for the history atom
//! `a card left your graveyard this turn` (CR 400.7): the one event-history
//! count over the departure records the graveyard publication writes.
use super::*;

fn parsed_rule(name: &str, oracle: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Artifact Creature \u{2014} Construct".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    let rule = parsed
        .abilities
        .into_iter()
        .find_map(|ability| ability.rule)
        .expect("one rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

/// The first event count of `value`: the lowered history atom.
fn event_count(value: &Value) -> Option<Value> {
    match value {
        Value::Object(map) if map.get("kind") == Some(&json!("countEventsThisTurn")) => {
            Some(value.clone())
        }
        Value::Object(map) => map.values().find_map(event_count),
        Value::Array(items) => items.iter().find_map(event_count),
        _ => None,
    }
}

fn flying_while(condition: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "condition": {
            "kind": "compare",
            "operator": ">=",
            "left": condition,
            "right": { "kind": "integer", "value": 1 },
        },
        "modifiers": [
            { "kind": "grantKeyword", "objects": { "kind": "self" }, "keyword": "flying" },
        ],
    })
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_card_leaving_its_owners_graveyard_is_recorded_for_that_owner() {
    // Relic Retriever's intervening condition.
    let rule = parsed_rule(
        "Relic Retriever",
        "At the beginning of each end step, if a card left your graveyard this turn, create a Treasure token.",
    );
    let count = event_count(&rule).expect("the condition counts history events");
    assert_eq!(count["event"], "cardLeftGraveyard", "{rule:#}");
    let mut engine = test_engine(2);
    let mut definition = test_definition("retriever", "Artifact Creature - Construct");
    definition.rules = vec![flying_while(count)];
    engine.state.players[0]
        .battlefield
        .push(test_instance("retriever", definition, "player-0"));
    let holds = |engine: &GameEngine| {
        let source = engine.state.players[0].battlefield[0].clone();
        engine.permanent_has_effective_keyword(&source, "flying")
    };
    assert!(!holds(&engine), "nothing left a graveyard yet");
    // An opponent's card leaving that opponent's graveyard is not ours.
    let theirs = test_instance("theirs", test_definition("theirs", "Instant"), "player-1");
    engine.state.players[1].exile.push(theirs);
    engine.enqueue_cards_left_graveyard_triggers("player-1", &["theirs".to_string()]);
    assert!(!holds(&engine), "another player's graveyard");
    let ours = test_instance("ours", test_definition("ours", "Sorcery"), "player-0");
    engine.state.players[0].exile.push(ours);
    engine.enqueue_cards_left_graveyard_triggers("player-0", &["ours".to_string()]);
    assert!(holds(&engine), "a card left the controller's graveyard");
}
