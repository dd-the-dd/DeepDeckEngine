use deepdeck_runtime_fixture_runner::{FixtureStatus, RuntimeFixture, run_fixture};
use serde_json::json;

fn fixture(assertions: serde_json::Value) -> serde_json::Value {
    json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "event-count-filter-assertion",
        "rule": {
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "drawCards",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "count": { "kind": "integer", "value": 1 }
            }]
        },
        "initialState": {
            "activePlayerId": "player-0",
            "step": "upkeep",
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "library": [{
                        "id": "drawn-card",
                        "name": "Drawn Card",
                        "typeLine": "Artifact"
                    }],
                    "hand": [{
                        "id": "draw-source",
                        "name": "Draw Source",
                        "typeLine": "Instant"
                    }]
                },
                { "id": "player-1", "name": "Opponent" }
            ]
        },
        "execution": {
            "controllerId": "player-0",
            "sourceDefinitionId": "draw-source"
        },
        "assertions": assertions
    })
}

#[test]
fn event_count_assertion_filters_by_player_and_step() {
    let parsed: RuntimeFixture = serde_json::from_value(fixture(json!([
        {
            "kind": "eventCount",
            "eventKind": "cardDrawn",
            "playerId": "player-0",
            "step": "upkeep",
            "equals": 1
        },
        {
            "kind": "eventCount",
            "eventKind": "cardDrawn",
            "playerId": "player-1",
            "step": "upkeep",
            "equals": 0
        },
        {
            "kind": "eventCount",
            "eventKind": "cardDrawn",
            "playerId": "player-0",
            "step": "draw",
            "equals": 0
        }
    ])))
    .expect("strict filtered event-count assertion schema parses");

    let result = run_fixture(parsed);

    assert!(matches!(result.status, FixtureStatus::Passed));
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

#[test]
fn event_count_filters_are_optional_for_existing_fixtures() {
    let parsed: RuntimeFixture = serde_json::from_value(fixture(json!([{
        "kind": "eventCount",
        "eventKind": "cardDrawn",
        "equals": 1
    }])))
    .expect("unfiltered event-count assertion remains valid");

    let result = run_fixture(parsed);

    assert!(matches!(result.status, FixtureStatus::Passed));
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

#[test]
fn event_count_assertion_rejects_unknown_fields() {
    let parsed = serde_json::from_value::<RuntimeFixture>(fixture(json!([{
        "kind": "eventCount",
        "eventKind": "cardDrawn",
        "playerId": "player-0",
        "step": "upkeep",
        "equals": 1,
        "unexpected": true
    }])));

    assert!(parsed.is_err());
}
