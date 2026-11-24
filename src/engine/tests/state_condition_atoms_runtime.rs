//! Runtime evaluation of the player-designation, turn and zone atoms of the
//! condition language (`state_condition_atoms.rs`): the monarch (CR 724),
//! the starting player (CR 103.1), whose turn it is (CR 500.1) and the zone
//! an object is in (CR 400.1), through the continuous evaluator, a player
//! quantifier and the resolution evaluator.
use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn source_card(engine: &GameEngine) -> CardInstance {
    engine.state.players[0].battlefield[0].clone()
}

fn engine_with_source() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(test_instance(
        "condition-source",
        test_definition("condition-source", "Creature - Test"),
        "player-0",
    ));
    engine
}

fn holds_for_source(engine: &GameEngine, condition: &Value) -> bool {
    let source = source_card(engine);
    assert!(condition_supported(condition), "{condition}");
    engine.condition_value_with_source(condition, "player-0", &BTreeMap::new(), Some(&source))
}

fn ability_on_stack(engine: &GameEngine, decisions: BTreeMap<String, Value>) -> StackObject {
    StackObject {
        id: "stack:state-condition".to_string(),
        controller: "player-0".to_string(),
        card: source_card(engine),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: None,
        decisions,
        targets: BTreeMap::new(),
    }
}

fn holds_at_resolution(engine: &GameEngine, condition: &Value, stack: &StackObject) -> bool {
    assert!(condition_supported(condition), "{condition}");
    engine.runtime_condition_value(condition, stack, &BTreeMap::new(), &stack.decisions)
}

#[test]
fn the_monarch_atom_reads_the_one_monarch_designation_for_each_player_reference() {
    let mut engine = engine_with_source();
    let you = json!({ "kind": "playerIsMonarch", "player": controller() });
    let an_opponent = json!({
        "kind": "playerQuantifier",
        "quantifier": "some",
        "players": { "kind": "opponentsOf", "player": controller() },
        "condition": { "kind": "playerIsMonarch", "player": { "kind": "quantifiedPlayer" } },
    });
    assert!(
        !holds_for_source(&engine, &you),
        "nobody is the monarch yet"
    );
    assert!(!holds_for_source(&engine, &an_opponent));

    engine.become_monarch("player-1");
    assert!(!holds_for_source(&engine, &you));
    assert!(holds_for_source(&engine, &an_opponent));

    // CR 724.4: a new monarch replaces the previous one.
    engine.become_monarch("player-0");
    assert!(holds_for_source(&engine, &you));
    assert!(!holds_for_source(&engine, &an_opponent));

    // At resolution, the player the triggering event records.
    let event_player =
        json!({ "kind": "playerIsMonarch", "player": { "kind": "triggeringPlayer" } });
    let stack = ability_on_stack(
        &engine,
        BTreeMap::from([("triggeringPlayerId".to_string(), json!("player-1"))]),
    );
    assert!(!holds_at_resolution(&engine, &event_player, &stack));
    engine.become_monarch("player-1");
    assert!(holds_at_resolution(&engine, &event_player, &stack));
}

#[test]
fn the_starting_player_is_a_fact_of_the_game_not_of_the_current_turn() {
    let mut engine = engine_with_source();
    let you = json!({ "kind": "playerWasStartingPlayer", "player": controller() });
    let not_you = json!({ "kind": "not", "operand": you.clone() });
    assert!(
        holds_for_source(&engine, &you),
        "player-0 took the first turn"
    );
    // Another player's turn does not make them the starting player.
    engine.state.active_player = 1;
    assert!(holds_for_source(&engine, &you));
    assert!(!engine.condition_value_with_source(
        &not_you,
        "player-0",
        &BTreeMap::new(),
        Some(&source_card(&engine)),
    ));
    // The quantified member is the reference player.
    let an_opponent = json!({
        "kind": "playerQuantifier",
        "quantifier": "some",
        "players": { "kind": "opponentsOf", "player": controller() },
        "condition": {
            "kind": "playerWasStartingPlayer",
            "player": { "kind": "quantifiedPlayer" },
        },
    });
    assert!(!holds_for_source(&engine, &an_opponent));
}

#[test]
fn another_players_turn_reads_the_active_player_against_the_event_player() {
    let mut engine = engine_with_source();
    let their_turn = json!({ "kind": "isPlayersTurn", "player": { "kind": "triggeringPlayer" } });
    let stack = ability_on_stack(
        &engine,
        BTreeMap::from([("triggeringPlayerId".to_string(), json!("player-1"))]),
    );
    engine.state.active_player = 0;
    assert!(!holds_at_resolution(&engine, &their_turn, &stack));
    engine.state.active_player = 1;
    assert!(holds_at_resolution(&engine, &their_turn, &stack));
    // Without a recorded event player the atom is false, never the
    // controller's turn by default.
    let unrecorded = ability_on_stack(&engine, BTreeMap::new());
    assert!(!holds_at_resolution(&engine, &their_turn, &unrecorded));
}

#[test]
fn an_object_zone_atom_reads_the_current_zone_of_the_source_instance() {
    let mut engine = engine_with_source();
    let on_battlefield = json!({
        "kind": "objectInZone",
        "object": { "kind": "self" },
        "zones": ["battlefield"],
    });
    let command_or_battlefield = json!({
        "kind": "objectInZone",
        "object": { "kind": "self" },
        "zones": ["commandZone", "battlefield"],
    });
    let in_graveyard = json!({
        "kind": "objectInZone",
        "object": { "kind": "self" },
        "zones": ["graveyard"],
    });
    let stack = ability_on_stack(&engine, BTreeMap::new());
    assert!(holds_at_resolution(&engine, &on_battlefield, &stack));
    assert!(holds_at_resolution(
        &engine,
        &command_or_battlefield,
        &stack
    ));
    assert!(!holds_at_resolution(&engine, &in_graveyard, &stack));

    let departed = engine.state.players[0].battlefield.remove(0);
    engine.state.players[0].graveyard.push(departed);
    assert!(!holds_at_resolution(&engine, &on_battlefield, &stack));
    assert!(holds_at_resolution(&engine, &in_graveyard, &stack));
}

#[test]
fn malformed_state_atoms_are_refused_by_the_validator() {
    for condition in [
        json!({ "kind": "playerIsMonarch" }),
        json!({ "kind": "playerIsMonarch", "player": { "kind": "eachPlayer" } }),
        json!({ "kind": "objectInZone", "object": { "kind": "self" }, "zones": [] }),
        json!({ "kind": "objectInZone", "object": { "kind": "self" }, "zones": ["sideboard"] }),
    ] {
        assert!(!condition_supported(&condition), "{condition}");
    }
}

/// Rite of the Serpent (`Destroy target creature. If that creature had a
/// +1/+1 counter on it, ...`): the departed target is read from the state
/// its departure recorded (CR 608.2h), never from the card in its new zone,
/// whose counters are gone.
#[test]
fn a_departed_target_answers_its_last_known_counters_and_combat_state() {
    let mut engine = test_engine(2);
    let mut creature = test_instance(
        "destroyed-creature",
        test_definition("destroyed-creature", "Creature - Snake"),
        "player-1",
    );
    creature.counters.insert("+1/+1".to_string(), 1);
    creature.flags.insert("attacking".to_string(), true);
    let mut graveyard_card = creature.clone();
    graveyard_card.counters.clear();
    graveyard_card.flags.clear();
    engine.state.players[1].graveyard.push(graveyard_card);
    let stack_object = StackObject {
        id: "stack:rite".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "rite-source",
            test_definition("rite-source", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "target-0".to_string(),
            TargetRef::Permanent {
                instance_id: "destroyed-creature".to_string(),
            },
        )]),
    };
    let had_counter = json!({
        "kind": "departedObjectMatches",
        "object": { "kind": "chosenTarget", "id": "target-0" },
        "where": { "kind": "hasCounter", "counter": "+1/+1" },
    });
    let was_attacking = json!({
        "kind": "departedObjectMatches",
        "object": { "kind": "chosenTarget", "id": "target-0" },
        "where": { "kind": "isAttacking" },
    });
    assert!(event_history::departed_object_condition_supported(
        &had_counter
    ));
    assert!(event_history::departed_object_condition_supported(
        &was_attacking
    ));
    let holds = |engine: &GameEngine, condition: &Value| {
        engine.runtime_condition_value(condition, &stack_object, &BTreeMap::new(), &BTreeMap::new())
    };
    // A departure record without the state answers nothing: false.
    engine.record_event(
        "permanentLeftBattlefield",
        Some("player-1".to_string()),
        Some("destroyed-creature".to_string()),
        json!({
            "destination": "graveyard",
            "typeLine": "Creature - Snake",
            "wasCreature": true,
            "owner": "player-1",
        }),
    );
    assert!(!holds(&engine, &had_counter));
    engine.record_event(
        "permanentLeftBattlefield",
        Some("player-1".to_string()),
        Some("destroyed-creature".to_string()),
        json!({
            "destination": "graveyard",
            "typeLine": "Creature - Snake",
            "wasCreature": true,
            "owner": "player-1",
            "lastKnownState": state_condition_atoms::departure_state_record(&creature),
        }),
    );
    assert!(holds(&engine, &had_counter));
    assert!(holds(&engine, &was_attacking));
    let mut two_counters = had_counter.clone();
    two_counters["where"]["minimum"] = json!(2);
    assert!(!holds(&engine, &two_counters));
    let mut no_counters = had_counter.clone();
    no_counters["where"] = json!({ "kind": "hasCounter", "minimum": 0, "maximum": 0 });
    assert!(!holds(&engine, &no_counters));
}
