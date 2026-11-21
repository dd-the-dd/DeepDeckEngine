use deepdeck_runtime_fixture_runner::{FixtureStatus, RuntimeFixture, run_fixture};
use serde_json::json;

#[test]
fn activate_ability_action_accepts_a_source_in_its_declared_hand_zone() {
    let fixture: RuntimeFixture = serde_json::from_value(json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "hand-activation-zone",
        "rule": {
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "activationZone": "hand",
            "costs": [{ "kind": "discardCard", "card": { "kind": "self" } }],
            "effects": [{
                "kind": "gainLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 1 }
            }]
        },
        "initialState": {
            "activePlayerId": "player-0",
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "hand": [{
                        "id": "hand-ability-source",
                        "name": "Hand Ability Source",
                        "typeLine": "Land"
                    }]
                },
                { "id": "player-1", "name": "Opponent" }
            ]
        },
        "actions": [
            {
                "kind": "activateAbility",
                "controllerId": "player-0",
                "sourceDefinitionId": "hand-ability-source",
                "abilityRuleIndex": 0
            },
            { "kind": "resolveTopStack" }
        ],
        "assertions": [
            {
                "kind": "zoneContains",
                "playerId": "player-0",
                "zone": "graveyard",
                "definitionId": "hand-ability-source",
                "equals": true
            },
            { "kind": "lifeTotal", "playerId": "player-0", "equals": 21 }
        ]
    }))
    .expect("strict hand activation fixture schema parses");

    let result = run_fixture(fixture);

    assert!(matches!(result.status, FixtureStatus::Passed));
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

fn battlefield_activation_initially_in_hand() -> serde_json::Value {
    json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1", "id": "dynamic-activation-zone",
        "rule": {
            "kind": "activatedAbility", "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "effects": [{ "kind": "gainLife", "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                          "amount": { "kind": "integer", "value": 1 } }]
        },
        "initialState": { "activePlayerId": "player-0", "players": [
            { "id": "player-0", "name": "Pilot", "hand": [
                { "id": "activation-source", "name": "Source", "typeLine": "Artifact" }
            ] },
            { "id": "player-1", "name": "Opponent" }
        ] },
        "actions": [{ "kind": "activateAbility", "controllerId": "player-0", "sourceDefinitionId": "activation-source" }],
        "assertions": [{ "kind": "stackDepth", "equals": 0 }]
    })
}

#[test]
fn engine_rejects_the_current_wrong_zone_after_structural_preflight() {
    let fixture = serde_json::from_value(battlefield_activation_initially_in_hand())
        .expect("fixture schema is valid");
    let result = run_fixture(fixture);
    assert!(matches!(result.status, FixtureStatus::Failed));
    assert!(result.error.as_deref().is_some_and(|error| {
        error.contains("canonical activated ability is not currently legal")
    }));
}

#[test]
fn structural_preflight_still_rejects_duplicate_initial_card_ids() {
    let mut value = battlefield_activation_initially_in_hand();
    value["initialState"]["players"][0]["library"] =
        value["initialState"]["players"][0]["hand"].clone();
    let fixture = serde_json::from_value(value).expect("fixture schema is valid");
    let result = run_fixture(fixture);
    assert!(matches!(result.status, FixtureStatus::Failed));
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| { error.contains("card ids must be non-empty and unique") })
    );
}
