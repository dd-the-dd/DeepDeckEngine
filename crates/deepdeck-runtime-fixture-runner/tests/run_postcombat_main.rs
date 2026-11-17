use deepdeck_engine_runtime::engine::GameStep;
use deepdeck_runtime_fixture_runner::{
    FIXTURE_SCHEMA_VERSION, FixtureAction, FixtureAssertion, FixturePlayer, FixtureState,
    FixtureStatus, RuntimeFixture, run_fixture,
};
use serde_json::json;

fn empty_player(id: &str) -> FixturePlayer {
    FixturePlayer {
        id: id.to_string(),
        name: id.to_string(),
        life: 20,
        library: Vec::new(),
        hand: Vec::new(),
        battlefield: Vec::new(),
        graveyard: Vec::new(),
        mana_pool: Vec::new(),
    }
}

#[test]
fn run_postcombat_main_rejects_a_fixture_at_another_step() {
    let result = run_fixture(RuntimeFixture {
        schema_version: FIXTURE_SCHEMA_VERSION.to_string(),
        id: "run-postcombat-main-requires-matching-step".to_string(),
        seed: 1,
        rule: json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [],
        }),
        initial_state: FixtureState {
            active_player_id: "player-0".to_string(),
            turn_number: 1,
            step: GameStep::PrecombatMain,
            players: vec![empty_player("player-0"), empty_player("player-1")],
        },
        execution: None,
        actions: vec![FixtureAction::RunPostcombatMain {
            maximum_turn_number: 1,
        }],
        decisions: Vec::new(),
        assertions: vec![FixtureAssertion::DecisionCount { equals: 0 }],
    });

    assert!(matches!(result.status, FixtureStatus::Failed));
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("requires initialState.step postcombatMain"))
    );
}
