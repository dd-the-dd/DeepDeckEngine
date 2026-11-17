use super::*;
use crate::{
    FixtureAssertion, FixtureState, ScriptedProvider, build_engine, permanent_by_definition,
};
use serde_json::json;
use std::collections::{BTreeSet, VecDeque};

fn twins() -> GameState {
    let setup: FixtureState = serde_json::from_value(json!({
        "activePlayerId": "player-0",
        "players": [
            {"id": "player-0", "name": "Controller", "battlefield": [
                {"id": "twin", "name": "Twin", "typeLine": "Creature", "power": "2", "toughness": "2", "rules": []}
            ]},
            {"id": "player-1", "name": "Other"}
        ]
    })).unwrap();
    let mut provider = ScriptedProvider {
        decisions: VecDeque::new(),
        calls: 0,
    };
    let engine = build_engine(&setup, 17, &json!({
        "kind": "spellAbility", "source": {"kind": "self"},
        "effects": [{"kind": "gainLife", "player": {"kind": "abilityController"}, "amount": {"kind": "integer", "value": 0}}]
    }), &BTreeSet::new(), &mut provider).unwrap();
    let mut state = engine.state().clone();
    state.players[0].battlefield[0]
        .counters
        .insert("+1/+1".to_string(), 1);
    let mut twin = state.players[0].battlefield[0].clone();
    twin.instance_id = "second-twin".to_string();
    state.players[0].battlefield.push(twin);
    state
}

fn expectation() -> PermanentGroupExpectation {
    serde_json::from_value(json!({
        "playerId": "player-0", "definitionId": "twin", "count": 2,
        "controller": "player-0", "owner": "player-0",
        "powerToughness": {"power": 3, "toughness": 3},
        "counters": {"+1/+1": 1, "ice": 0}
    }))
    .unwrap()
}

#[test]
fn schema_requires_positive_cardinality_and_typed_known_properties() {
    let valid = json!({"kind": "permanentGroup", "playerId": "player-0", "definitionId": "twin", "count": 2});
    assert!(matches!(
        serde_json::from_value::<FixtureAssertion>(valid.clone()).unwrap(),
        FixtureAssertion::PermanentGroup(_)
    ));
    for malformed in [
        json!({"kind": "permanentGroup", "playerId": "player-0", "definitionId": "twin"}),
        json!({"kind": "permanentGroup", "playerId": "player-0", "definitionId": "twin", "count": 0}),
        json!({"kind": "permanentGroup", "playerId": "player-0", "definitionId": "twin", "count": 2, "controller": 0}),
        json!({"kind": "permanentGroup", "playerId": "player-0", "definitionId": "twin", "count": 2, "unknown": true}),
        json!({"kind": "permanentGroup", "playerId": "player-0", "definitionId": "twin", "count": 2, "powerToughness": {"power": 3, "toughness": 3, "unknown": 1}}),
    ] {
        assert!(serde_json::from_value::<FixtureAssertion>(malformed).is_err());
    }
}

#[test]
fn exact_group_checks_effective_stats_and_never_accepts_empty_or_wrong_cardinality() {
    let mut state = twins();
    assert!(evaluate_permanent_group(&state, &expectation()).0);
    let mut expected = expectation();
    expected.count = NonZeroUsize::new(1).unwrap();
    assert!(!evaluate_permanent_group(&state, &expected).0);
    expected.count = NonZeroUsize::new(3).unwrap();
    assert!(!evaluate_permanent_group(&state, &expected).0);
    state.players[0].battlefield.clear();
    assert!(!evaluate_permanent_group(&state, &expectation()).0);
}

#[test]
fn one_incorrect_twin_fails_even_when_the_first_twin_is_correct() {
    for property in ["controller", "owner", "power", "counter"] {
        let mut state = twins();
        let twin = &mut state.players[0].battlefield[1];
        match property {
            "controller" => twin.controller = "player-1".to_string(),
            "owner" => twin.owner = "player-1".to_string(),
            "power" => twin.definition.power = Some("1".to_string()),
            "counter" => {
                twin.counters.insert("ice".to_string(), 1);
            }
            _ => unreachable!(),
        }
        let (passed, diagnostic) = evaluate_permanent_group(&state, &expectation());
        assert!(!passed, "must inspect each member's {property}");
        assert!(diagnostic.contains("second-twin"));
    }
}

#[test]
fn singular_lookup_keeps_refusing_duplicate_definition_id() {
    let mut state = twins();
    assert!(permanent_by_definition(&state, "player-0", "twin").is_none());
    assert!(evaluate_permanent_group(&state, &expectation()).0);
    state.players[0].battlefield.pop();
    assert!(permanent_by_definition(&state, "player-0", "twin").is_some());
}
