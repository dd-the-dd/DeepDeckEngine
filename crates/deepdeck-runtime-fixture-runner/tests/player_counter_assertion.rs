use deepdeck_runtime_fixture_runner::{FixtureStatus, RuntimeFixture, run_fixture};
use serde_json::json;

fn fixture(assertion: serde_json::Value) -> serde_json::Value {
    json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "player-counter-assertion",
        "rule": {
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "addPlayerCounters",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "counter": "energy",
                "count": { "kind": "integer", "value": 3 }
            }]
        },
        "initialState": {
            "activePlayerId": "player-0",
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "hand": [{
                        "id": "counter-source",
                        "name": "Counter Source",
                        "typeLine": "Sorcery"
                    }]
                },
                { "id": "player-1", "name": "Opponent" }
            ]
        },
        "execution": {
            "controllerId": "player-0",
            "sourceDefinitionId": "counter-source"
        },
        "assertions": [assertion]
    })
}

#[test]
fn player_counter_assertion_reads_player_state_counters() {
    let parsed: RuntimeFixture = serde_json::from_value(fixture(json!({
        "kind": "playerCounter",
        "playerId": "player-0",
        "counter": "energy",
        "equals": 3
    })))
    .expect("generic player-counter assertion schema parses");

    let result = run_fixture(parsed);

    assert!(matches!(result.status, FixtureStatus::Passed));
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

#[test]
fn player_counter_assertion_rejects_unknown_fields() {
    let parsed = serde_json::from_value::<RuntimeFixture>(fixture(json!({
        "kind": "playerCounter",
        "playerId": "player-0",
        "counter": "energy",
        "equals": 3,
        "unexpected": true
    })));

    assert!(parsed.is_err());
}
