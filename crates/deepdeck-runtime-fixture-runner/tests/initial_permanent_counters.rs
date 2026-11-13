use deepdeck_runtime_fixture_runner::{FixtureStatus, RuntimeFixture, run_fixture};
use serde_json::json;

fn fixture(counter_count: i32) -> serde_json::Value {
    json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "initial-permanent-counters",
        "rule": {
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": []
        },
        "initialState": {
            "activePlayerId": "player-0",
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "hand": [{
                        "id": "fixture-source",
                        "name": "Fixture Source",
                        "typeLine": "Sorcery"
                    }]
                },
                {
                    "id": "player-1",
                    "name": "Opponent",
                    "battlefield": [{
                        "id": "fixture-planeswalker",
                        "name": "Fixture Planeswalker",
                        "typeLine": "Legendary Planeswalker - Test",
                        "counters": { "loyalty": counter_count }
                    }]
                }
            ]
        },
        "execution": {
            "controllerId": "player-0",
            "sourceDefinitionId": "fixture-source"
        },
        "assertions": [{
            "kind": "permanentCounter",
            "playerId": "player-1",
            "definitionId": "fixture-planeswalker",
            "counter": "loyalty",
            "equals": counter_count
        }]
    })
}

#[test]
fn initial_permanent_counters_decode_and_populate_state() {
    let parsed: RuntimeFixture =
        serde_json::from_value(fixture(7)).expect("initial permanent counters decode");
    assert_eq!(
        parsed.initial_state.players[1].battlefield[0]
            .counters
            .get("loyalty"),
        Some(&7)
    );

    let result = run_fixture(parsed);

    assert!(matches!(result.status, FixtureStatus::Passed));
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

#[test]
fn initial_permanent_counters_reject_negative_values() {
    let parsed: RuntimeFixture =
        serde_json::from_value(fixture(-1)).expect("counter schema remains generic");

    let result = run_fixture(parsed);

    assert!(matches!(result.status, FixtureStatus::Failed));
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("cannot be negative"))
    );
}
