use deepdeck_runtime_fixture_runner::{FixtureStatus, RuntimeFixture, run_fixture};
use serde_json::json;

#[test]
fn play_land_action_uses_the_normal_land_play_path() {
    let fixture: RuntimeFixture = serde_json::from_value(json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "generic-play-land-action",
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
                    "name": "Land player",
                    "hand": [{
                        "id": "fixture-land",
                        "name": "Fixture Land",
                        "typeLine": "Land"
                    }]
                },
                { "id": "player-1", "name": "Opponent" }
            ]
        },
        "actions": [{
            "kind": "playLand",
            "playerId": "player-0",
            "sourceDefinitionId": "fixture-land"
        }],
        "assertions": [
            {
                "kind": "zoneContains",
                "playerId": "player-0",
                "zone": "battlefield",
                "definitionId": "fixture-land",
                "equals": true
            },
            {
                "kind": "eventCount",
                "eventKind": "landPlayed",
                "playerId": "player-0",
                "equals": 1
            },
            { "kind": "stackDepth", "equals": 0 }
        ]
    }))
    .expect("generic playLand fixture schema parses");

    let result = run_fixture(fixture);

    assert!(matches!(result.status, FixtureStatus::Passed), "{result:?}");
}
