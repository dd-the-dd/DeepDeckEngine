use super::*;
use serde_json::json;

fn fixture(quantity: u32, actions: Value, assertions: Value) -> RuntimeFixture {
    serde_json::from_value(json!({
        "schemaVersion":FIXTURE_SCHEMA_VERSION,"id":"created-action-source",
        "rule":{"kind":"spellAbility","source":{"kind":"self"},"effects":[
            {"kind":"createTokens","controller":{"kind":"abilityController"},
             "quantity":{"kind":"integer","value":quantity},"token":{"kind":"namedToken","name":"Clue"}}
        ]},
        "initialState":{"activePlayerId":"p0","players":[
            {"id":"p0","name":"P0","manaPool":["C","C"],
             "battlefield":[{"id":"creator","name":"Creator","typeLine":"Artifact","rules":[]}],
             "library":[{"id":"drawn","name":"Drawn","typeLine":"Instant","rules":[]}]},
            {"id":"p1","name":"P1","library":[{"id":"other","name":"Other","typeLine":"Sorcery","rules":[]}]}
        ]},"actions":actions,"assertions":assertions
    })).unwrap()
}
fn create() -> Value {
    json!({"kind":"executeRule","execution":{"controllerId":"p0","sourceDefinitionId":"creator"}})
}

#[test]
fn newly_created_source_uses_the_existing_public_activation_path() {
    let result = run_fixture(fixture(
        1,
        json!([
            create(),{"kind":"activateAbility","controllerId":"p0","sourceDefinitionId":"token:clue"},
            {"kind":"resolveTopStack"}
        ]),
        json!([
            {"kind":"zoneContains","playerId":"p0","zone":"hand","definitionId":"drawn","equals":true},
            {"kind":"zoneCount","playerId":"p0","zone":"battlefield","equals":1},
            {"kind":"manaPoolEquals","playerId":"p0","symbols":[]},
            {"kind":"eventCount","eventKind":"tokenCreated","equals":1},
            {"kind":"eventCount","eventKind":"permanentSacrificed","equals":1},
            {"kind":"eventCount","eventKind":"cardDrawn","equals":1},
            {"kind":"stackDepth","equals":0}
        ]),
    ));
    assert!(matches!(result.status, FixtureStatus::Passed), "{result:?}");
}

#[test]
fn current_world_source_errors_preserve_actor_uniqueness_and_ability_role() {
    for (quantity, actor, kind, source, error) in [
        (
            0,
            "p0",
            "activateAbility",
            "missing",
            "absent from p0's supported activation zones",
        ),
        (
            2,
            "p0",
            "activateAbility",
            "token:clue",
            "ambiguous across p0's supported activation zones",
        ),
        (
            1,
            "p1",
            "activateAbility",
            "token:clue",
            "absent from p1's supported activation zones",
        ),
        (
            1,
            "p0",
            "activateManaAbility",
            "token:clue",
            "canonical mana ability is not currently legal",
        ),
    ] {
        let mut actions = Vec::new();
        if quantity > 0 {
            actions.push(create());
        }
        actions.push(json!({"kind":"probeFailure","action":{
            "kind":kind,"controllerId":actor,"sourceDefinitionId":source},
            "expectedErrorContains":error}));
        let result = run_fixture(fixture(
            quantity,
            Value::Array(actions),
            json!([
                {"kind":"manaPoolEquals","playerId":"p0","symbols":["C","C"]},
                {"kind":"eventCount","eventKind":"tokenCreated","equals":quantity},
                {"kind":"eventCount","eventKind":"permanentSacrificed","equals":0},
                {"kind":"eventCount","eventKind":"cardDrawn","equals":0},
                {"kind":"decisionCount","equals":0},{"kind":"stackDepth","equals":0}
            ]),
        ));
        assert!(
            matches!(result.status, FixtureStatus::Passed),
            "{kind}/{actor}/{quantity}: {result:?}"
        );
    }
}
