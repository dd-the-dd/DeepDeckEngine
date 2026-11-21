use deepdeck_runtime_fixture_runner::{FixtureStatus, RuntimeFixture, run_fixture};
use serde_json::json;

fn delve_fixture(
    id: &str,
    declaration_decisions: Option<serde_json::Value>,
    mana_pool: &[&str],
) -> RuntimeFixture {
    let mut cast_action = json!({
        "kind": "castSpell",
        "controllerId": "player-0",
        "sourceDefinitionId": "delve-spell"
    });
    if let Some(declaration_decisions) = declaration_decisions {
        cast_action["declarationDecisions"] = declaration_decisions;
    }
    serde_json::from_value(json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": id,
        "rule": {
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "delve" }
        },
        "initialState": {
            "activePlayerId": "player-0",
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "manaPool": mana_pool,
                    "hand": [{
                        "id": "delve-spell",
                        "name": "Delve Spell",
                        "typeLine": "Creature",
                        "manaCost": "{2}{U}",
                        "power": "2",
                        "toughness": "2"
                    }],
                    "graveyard": [
                        { "id": "delve-card-a", "name": "Delve Card A", "typeLine": "Instant" },
                        { "id": "delve-card-b", "name": "Delve Card B", "typeLine": "Creature" }
                    ]
                },
                { "id": "player-1", "name": "Opponent" }
            ]
        },
        "actions": [cast_action],
        "assertions": [{ "kind": "stackDepth", "equals": 0 }]
    }))
    .expect("strict declaration-decision fixture schema parses")
}

#[test]
fn cast_action_selects_an_existing_legal_declaration_by_requested_decisions() {
    let fixture: RuntimeFixture = serde_json::from_value(json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "cast-declaration-decisions",
        "rule": {
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "delve" }
        },
        "initialState": {
            "activePlayerId": "player-0",
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "manaPool": ["U"],
                    "hand": [{
                        "id": "delve-spell",
                        "name": "Delve Spell",
                        "typeLine": "Creature",
                        "manaCost": "{2}{U}",
                        "power": "2",
                        "toughness": "2"
                    }],
                    "graveyard": [
                        { "id": "delve-card-a", "name": "Delve Card A", "typeLine": "Instant" },
                        { "id": "delve-card-b", "name": "Delve Card B", "typeLine": "Creature" }
                    ]
                },
                { "id": "player-1", "name": "Opponent" }
            ]
        },
        "actions": [
            {
                "kind": "castSpell",
                "controllerId": "player-0",
                "sourceDefinitionId": "delve-spell",
                "declarationDecisions": {
                    "delveCards": [
                        "player-0:delve-card-a:1",
                        "player-0:delve-card-b:2"
                    ]
                }
            },
            { "kind": "resolveTopStack" }
        ],
        "assertions": [
            {
                "kind": "zoneContains",
                "playerId": "player-0",
                "zone": "battlefield",
                "definitionId": "delve-spell",
                "equals": true
            },
            {
                "kind": "zoneContains",
                "playerId": "player-0",
                "zone": "exile",
                "definitionId": "delve-card-a",
                "equals": true
            },
            {
                "kind": "zoneContains",
                "playerId": "player-0",
                "zone": "exile",
                "definitionId": "delve-card-b",
                "equals": true
            },
            { "kind": "manaPoolEquals", "playerId": "player-0", "symbols": [] },
            { "kind": "stackDepth", "equals": 0 }
        ]
    }))
    .expect("strict cast declaration-decisions fixture schema parses");

    let result = run_fixture(fixture);

    assert!(
        matches!(result.status, FixtureStatus::Passed),
        "unexpected fixture error: {:?}",
        result.error
    );
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

#[test]
fn cast_action_rejects_an_unknown_declaration_decision_id() {
    let result = run_fixture(delve_fixture(
        "cast-declaration-decisions-unknown-id",
        Some(json!({ "unknownDecision": true })),
        &["U"],
    ));

    assert!(matches!(result.status, FixtureStatus::Failed));
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("unknown declaration decision unknownDecision")),
        "unexpected fixture error: {:?}",
        result.error
    );
}

#[test]
fn cast_action_rejects_a_value_not_present_in_any_legal_declaration() {
    let result = run_fixture(delve_fixture(
        "cast-declaration-decisions-invalid-value",
        Some(json!({ "delveCards": ["not-in-the-graveyard"] })),
        &["U"],
    ));

    assert!(matches!(result.status, FixtureStatus::Failed));
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("canonical spell cast is not currently legal")),
        "unexpected fixture error: {:?}",
        result.error
    );
}

#[test]
fn absent_declaration_decisions_remain_valid_for_a_unique_cast() {
    let fixture: RuntimeFixture = serde_json::from_value(json!({
        "schemaVersion": "deepdeck-runtime-fixture/v1",
        "id": "cast-without-declaration-decisions",
        "rule": {
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "flying" }
        },
        "initialState": {
            "activePlayerId": "player-0",
            "players": [
                {
                    "id": "player-0",
                    "name": "Pilot",
                    "manaPool": ["U"],
                    "hand": [{
                        "id": "unique-spell",
                        "name": "Unique Spell",
                        "typeLine": "Creature",
                        "manaCost": "{U}",
                        "power": "1",
                        "toughness": "1"
                    }]
                },
                { "id": "player-1", "name": "Opponent" }
            ]
        },
        "actions": [
            {
                "kind": "castSpell",
                "controllerId": "player-0",
                "sourceDefinitionId": "unique-spell"
            },
            { "kind": "resolveTopStack" }
        ],
        "assertions": [{
            "kind": "zoneContains",
            "playerId": "player-0",
            "zone": "battlefield",
            "definitionId": "unique-spell",
            "equals": true
        }]
    }))
    .expect("the backward-compatible cast fixture schema parses");

    let result = run_fixture(fixture);

    assert!(
        matches!(result.status, FixtureStatus::Passed),
        "unexpected fixture error: {:?}",
        result.error
    );
    assert!(result.assertions.iter().all(|assertion| assertion.passed));
}

#[test]
fn an_underspecified_declaration_remains_an_explicit_ambiguity() {
    let result = run_fixture(delve_fixture(
        "cast-declaration-ambiguity",
        None,
        &["C", "U"],
    ));

    assert!(matches!(result.status, FixtureStatus::Failed));
    assert!(
        result.error.as_deref().is_some_and(|error| {
            error.contains("matching legal cast declarations")
                && error.contains("must disambiguate them")
        }),
        "unexpected fixture error: {:?}",
        result.error
    );
}
