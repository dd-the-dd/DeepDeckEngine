use deepdeck_runtime_fixture_runner::{FixtureStatus, RuntimeFixture, run_fixture};
use serde_json::json;

fn fixture(
    explicit_rules: Option<serde_json::Value>,
    expected_hand_count: usize,
) -> RuntimeFixture {
    let mut source = json!({
        "id": "fixture-source",
        "name": "Fixture Source",
        "typeLine": "Sorcery",
        "manaCost": "{0}"
    });
    if let Some(rules) = explicit_rules {
        source["rules"] = rules;
    }

    serde_json::from_value(json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "explicit-empty-supporting-rules",
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
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "library": [{ "id": "draw", "name": "Draw", "typeLine": "Land" }],
                    "hand": [source]
                },
                {
                    "id": "player-1",
                    "name": "Opponent"
                }
            ]
        },
        "actions": [
            { "kind": "castSpell", "controllerId": "player-0", "sourceDefinitionId": "fixture-source" },
            { "kind": "resolveTopStack" }
        ],
        "assertions": [{
            "kind": "zoneCount",
            "playerId": "player-0",
            "zone": "hand",
            "equals": expected_hand_count
        }]
    }))
    .expect("fixture schema decodes")
}

#[test]
fn missing_rules_injects_the_fixture_rule_for_action_sources() {
    let result = run_fixture(fixture(None, 1));

    assert!(matches!(result.status, FixtureStatus::Passed));
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

#[test]
fn explicit_empty_rules_prevents_fixture_rule_injection() {
    let result = run_fixture(fixture(Some(json!([])), 0));

    assert!(matches!(result.status, FixtureStatus::Passed));
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}
