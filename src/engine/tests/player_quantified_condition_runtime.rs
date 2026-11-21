//! `playerQuantifier` (CR 102.2-102.3): a player-state proposition evaluated
//! for each member of a player population. These tests drive the condition
//! of a static ability and the intervening-if guard of a triggered ability
//! (CR 603.4) through live game state, including a guard that becomes false
//! between the trigger and its resolution.
use super::*;

fn quantified_player() -> Value {
    json!({ "kind": "quantifiedPlayer" })
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn opponents() -> Value {
    json!({ "kind": "opponentsOf", "player": controller() })
}

fn quantified(quantifier: &str, players: Value, condition: Value) -> Value {
    json!({ "kind": "playerQuantifier", "quantifier": quantifier,
        "players": players, "condition": condition })
}

fn compare(operator: &str, left: Value, right: i64) -> Value {
    json!({"kind":"compare","operator":operator,"left":left,
        "right":{"kind":"integer","value":right}})
}

fn life_at_most(value: i64) -> Value {
    compare(
        "<=",
        json!({ "kind": "lifeTotal", "player": quantified_player() }),
        value,
    )
}

fn engine_with_static_source(condition: Value) -> GameEngine {
    let mut engine = test_engine(3);
    let mut definition = test_definition("quantified-source", "Creature - Test");
    definition.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "condition": condition,
        "modifiers": [{
            "kind": "modifyPowerToughness",
            "objects": { "kind": "self" },
            "power": 1,
            "toughness": 0,
        }],
    })];
    engine.state.players[0].battlefield.push(test_instance(
        "quantified-source",
        definition,
        "player-0",
    ));
    engine
}

fn holds(engine: &GameEngine) -> bool {
    let source = engine.state.players[0].battlefield[0].clone();
    GameEngine::effective_power_toughness(&engine.state)[&source.instance_id].current_power == 3
}

#[test]
fn quantified_shapes_are_validated_before_execution() {
    assert!(condition_supported(&quantified(
        "some",
        opponents(),
        life_at_most(10)
    )));
    assert!(condition_supported(&quantified(
        "every",
        json!({ "kind": "eachPlayer" }),
        life_at_most(10)
    )));
    // An unknown quantifier or an anchor other than the controller has no evaluator.
    assert!(!condition_supported(&quantified(
        "most",
        opponents(),
        life_at_most(10)
    )));
    assert!(!condition_supported(&quantified(
        "some",
        json!({ "kind": "opponentsOf", "player": { "kind": "triggeringPlayer" } }),
        life_at_most(10)
    )));
    // A proposition that names another player would be read against the
    // bound member: it is refused instead of silently re-anchored.
    assert!(!condition_supported(&quantified(
        "some",
        opponents(),
        compare(
            "<=",
            json!({ "kind": "lifeTotal", "player": controller() }),
            10
        )
    )));
    // The inner proposition still goes through the shared validator.
    assert!(!condition_supported(&quantified(
        "some",
        opponents(),
        compare(
            "<=",
            json!({ "kind": "lifeTotalOfNobody", "player": quantified_player() }),
            10
        )
    )));
}

#[test]
fn some_opponent_reads_each_opponent_and_never_the_controller() {
    let mut engine = engine_with_static_source(quantified("some", opponents(), life_at_most(10)));
    assert!(!holds(&engine), "every player starts at 20 life");
    engine.state.players[0].life = 5;
    assert!(!holds(&engine), "the controller is not an opponent");
    engine.state.players[2].life = 10;
    assert!(
        holds(&engine),
        "the second opponent satisfies the proposition"
    );
    engine.state.players[2].has_lost = true;
    assert!(!holds(&engine), "a player who left the game is not counted");
}

#[test]
fn every_player_includes_the_controller() {
    let mut engine = engine_with_static_source(quantified(
        "every",
        json!({ "kind": "eachPlayer" }),
        life_at_most(10),
    ));
    engine.state.players[1].life = 10;
    engine.state.players[2].life = 3;
    assert!(!holds(&engine), "the controller still has 20 life");
    engine.state.players[0].life = 10;
    assert!(holds(&engine));
}

#[test]
fn no_opponent_is_the_negated_existential_over_controlled_permanents() {
    let creature = compare(
        ">=",
        json!({ "kind": "countPermanents", "player": quantified_player(),
            "where": { "kind": "cardTypeContains", "value": "Creature" },
            "excludeSource": false }),
        1,
    );
    let mut engine = engine_with_static_source(json!({
        "kind": "not",
        "operand": quantified("some", opponents(), creature),
    }));
    assert!(holds(&engine), "only the controller controls a creature");
    engine.state.players[1].battlefield.push(test_instance(
        "opponent-land",
        test_definition("opponent-land", "Land"),
        "player-1",
    ));
    assert!(holds(&engine), "a land is not a creature");
    engine.state.players[2].battlefield.push(test_instance(
        "opponent-creature",
        test_definition("opponent-creature", "Creature - Bear"),
        "player-2",
    ));
    assert!(!holds(&engine));
}

#[test]
fn zone_and_counter_measures_are_read_from_the_bound_player() {
    let graveyard = compare(
        ">=",
        json!({ "kind": "countCards",
            "zone": { "kind": "graveyard", "player": quantified_player() },
            "where": { "kind": "cardTypeContains", "value": "Creature" } }),
        2,
    );
    let mut engine = engine_with_static_source(quantified("some", opponents(), graveyard));
    for index in 0..2 {
        engine.state.players[0].graveyard.push(test_instance(
            &format!("own-dead-{index}"),
            test_definition("own-dead", "Creature - Bear"),
            "player-0",
        ));
    }
    assert!(
        !holds(&engine),
        "the controller's graveyard is not an opponent's"
    );
    engine.state.players[1].graveyard.push(test_instance(
        "dead-1",
        test_definition("dead", "Creature - Bear"),
        "player-1",
    ));
    engine.state.players[2].graveyard.push(test_instance(
        "dead-2",
        test_definition("dead", "Creature - Bear"),
        "player-2",
    ));
    assert!(
        !holds(&engine),
        "two opponents with one card each is not one with two"
    );
    engine.state.players[2].graveyard.push(test_instance(
        "dead-3",
        test_definition("dead", "Creature - Bear"),
        "player-2",
    ));
    assert!(holds(&engine));

    let poison = compare(
        ">=",
        json!({ "kind": "countPlayerCounters", "player": quantified_player(), "counter": "poison" }),
        3,
    );
    let mut engine = engine_with_static_source(quantified("some", opponents(), poison));
    engine.state.players[0].counters.insert("poison".into(), 9);
    assert!(!holds(&engine));
    engine.state.players[1].counters.insert("poison".into(), 3);
    assert!(holds(&engine));
}

#[test]
fn filtered_spell_history_is_read_for_the_bound_player() {
    let noncreature = quantified(
        "some",
        opponents(),
        json!({ "kind": "hasCastSpellThisTurn", "player": quantified_player(),
            "where": { "kind": "not", "operand": { "kind": "cardTypeContains", "value": "Creature" } } }),
    );
    let mut engine = engine_with_static_source(noncreature);
    let cast = |engine: &mut GameEngine, player: usize, id: &str, type_line: &str| {
        let owner = format!("player-{player}");
        engine.state.players[player].graveyard.push(test_instance(
            id,
            test_definition(id, type_line),
            &owner,
        ));
        engine.record_event("spellCast", Some(owner), Some(id.to_string()), json!({}));
    };
    cast(&mut engine, 0, "own-bolt", "Instant");
    assert!(
        !holds(&engine),
        "the controller's spell is not an opponent's"
    );
    cast(&mut engine, 1, "bear", "Creature - Bear");
    assert!(
        !holds(&engine),
        "a creature spell does not match the filter"
    );
    cast(&mut engine, 2, "bolt", "Instant");
    assert!(holds(&engine));
    engine.state.turn_number += 1;
    assert!(!holds(&engine), "spells of an earlier turn do not count");
}

fn guarded_trigger(engine: &mut GameEngine) -> CardInstance {
    let mut definition = test_definition("guarded-trigger", "Creature - Test");
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "stepBegan", "step": "upkeep", "player": controller() },
        "condition": quantified("some", opponents(), life_at_most(10)),
        "effects": [{
            "kind": "gainLife",
            "player": controller(),
            "amount": { "kind": "integer", "value": 3 },
        }],
    });
    definition.rules = vec![rule];
    let source = test_instance("guarded-trigger", definition, "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture source is admitted");
    let (player, index) = engine
        .permanent_position("guarded-trigger")
        .expect("source is on the battlefield");
    engine.state.players[player].battlefield[index].clone()
}

#[test]
fn intervening_if_is_checked_when_the_trigger_would_be_put_on_the_stack() {
    let mut engine = test_engine(2);
    let source = guarded_trigger(&mut engine);
    let rule = source.definition.rules[0].clone();
    engine.put_triggered_ability_on_stack(&source, rule);
    assert!(
        engine.state.stack.is_empty(),
        "no opponent has 10 or less life: the ability does not trigger"
    );
}

#[test]
fn intervening_if_is_checked_again_on_resolution() {
    for (opponent_life_at_resolution, gained) in [(10, true), (11, false)] {
        let mut engine = test_engine(2);
        let source = guarded_trigger(&mut engine);
        engine.state.players[1].life = 7;
        let rule = source.definition.rules[0].clone();
        engine.put_triggered_ability_on_stack(&source, rule);
        assert_eq!(
            engine.state.stack.len(),
            1,
            "the guard holds when triggered"
        );
        engine.state.players[1].life = opponent_life_at_resolution;
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the triggered ability resolves");
        assert_eq!(
            engine.state.players[0].life,
            if gained { 23 } else { 20 },
            "opponent at {opponent_life_at_resolution} life on resolution"
        );
    }
}
