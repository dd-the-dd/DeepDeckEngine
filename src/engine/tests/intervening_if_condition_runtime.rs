//! Conditions of an intervening-if guard (CR 603.4) and of a static ability
//! are one Boolean program over live game state. These tests exercise the
//! state propositions the shared Oracle condition atoms lower to: life total,
//! this-turn deaths filtered by type, and spell casts during the previous turn.
use super::*;

fn engine_with_source(condition: Value) -> GameEngine {
    let mut engine = test_engine(2);
    let mut definition = test_definition("guarded-source", "Creature - Test");
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
        "guarded-source",
        definition,
        "player-0",
    ));
    engine
}

fn holds(engine: &GameEngine) -> bool {
    let source = engine.state.players[0].battlefield[0].clone();
    GameEngine::effective_power_toughness(&engine.state)[&source.instance_id].current_power == 3
}

fn compare(operator: &str, left: Value, right: i64) -> Value {
    json!({"kind":"compare","operator":operator,"left":left,
        "right":{"kind":"integer","value":right}})
}

fn previous_turn_spells(aggregate: &str) -> Value {
    json!({"kind":"countEventsPreviousTurn","event":"spellCast","aggregate":aggregate})
}

fn record_cast(engine: &mut GameEngine, player: &str, turn: u32) {
    let current = engine.state.turn_number;
    engine.state.turn_number = turn;
    engine.record_event("spellCast", Some(player.to_string()), None, json!({}));
    engine.state.turn_number = current;
}

#[test]
fn lowered_guard_shapes_are_accepted_and_unknown_aggregates_refused() {
    assert!(condition_supported(&compare(
        "==",
        previous_turn_spells("total"),
        0
    )));
    assert!(condition_supported(&compare(
        ">=",
        previous_turn_spells("greatestPerPlayer"),
        2
    )));
    assert!(!condition_supported(&compare(
        ">=",
        previous_turn_spells("median"),
        2
    )));
    assert!(!condition_supported(&compare(
        ">=",
        json!({"kind":"countEventsPreviousTurn","aggregate":"total"}),
        1
    )));
}

#[test]
fn no_spells_last_turn_reads_only_the_immediately_preceding_turn() {
    let mut engine = engine_with_source(compare("==", previous_turn_spells("total"), 0));
    engine.state.turn_number = 5;
    assert!(holds(&engine), "no spell was cast during turn 4");

    record_cast(&mut engine, "player-1", 3);
    record_cast(&mut engine, "player-0", 5);
    assert!(
        holds(&engine),
        "turns 3 and the current turn are not last turn"
    );

    record_cast(&mut engine, "player-1", 4);
    assert!(
        !holds(&engine),
        "a spell cast during turn 4 breaks the condition"
    );

    engine.state.turn_number = 6;
    assert!(
        !holds(&engine),
        "turn 5 was the previous turn and had one cast"
    );

    engine.state.turn_number = 7;
    assert!(holds(&engine), "turn 6 had no cast");
}

#[test]
fn a_player_casting_two_spells_counts_per_player_not_in_total() {
    let mut engine =
        engine_with_source(compare(">=", previous_turn_spells("greatestPerPlayer"), 2));
    engine.state.turn_number = 5;
    record_cast(&mut engine, "player-0", 4);
    record_cast(&mut engine, "player-1", 4);
    assert!(
        !holds(&engine),
        "two spells by two players is not two by one player"
    );
    record_cast(&mut engine, "player-1", 4);
    assert!(holds(&engine), "one player cast two spells");
}

#[test]
fn first_turn_has_no_previous_turn() {
    let mut engine = engine_with_source(compare("==", previous_turn_spells("total"), 0));
    engine.state.turn_number = 0;
    assert!(holds(&engine));
}

#[test]
fn deaths_this_turn_are_filtered_by_card_type_across_all_players() {
    let died = |type_line: &str| json!({"typeLine": type_line});
    let mut engine = engine_with_source(compare(
        ">=",
        json!({"kind":"countEventsThisTurn","event":"permanentDied",
            "where":{"kind":"cardTypeContains","value":"Creature"}}),
        2,
    ));
    engine.record_event(
        "permanentDied",
        Some("player-1".into()),
        None,
        died("Artifact"),
    );
    engine.record_event(
        "permanentDied",
        Some("player-1".into()),
        None,
        died("Creature - Elf"),
    );
    assert!(!holds(&engine), "one creature and one artifact died");
    engine.record_event(
        "permanentDied",
        Some("player-0".into()),
        None,
        died("Creature - Human"),
    );
    assert!(
        holds(&engine),
        "an opponent's and the controller's creatures both count"
    );
    engine.state.turn_number += 1;
    assert!(!holds(&engine), "deaths of an earlier turn do not count");
}

#[test]
fn life_total_threshold_follows_the_controller_life() {
    let mut engine = engine_with_source(compare(
        "<=",
        json!({"kind":"lifeTotal","player":{"kind":"controllerOf","object":{"kind":"self"}}}),
        5,
    ));
    assert!(!holds(&engine), "starting life is above the threshold");
    engine.state.players[0].life = 5;
    assert!(holds(&engine));
    engine.state.players[1].life = 1;
    engine.state.players[0].life = 6;
    assert!(
        !holds(&engine),
        "an opponent's life is not the controller's"
    );
}

#[test]
fn attacked_this_turn_counts_the_controllers_declared_attackers() {
    let mut engine = engine_with_source(compare(
        ">=",
        json!({"kind":"countEventsThisTurn","event":"attackerDeclared",
            "player":{"kind":"controllerOf","object":{"kind":"self"}},"where":null}),
        1,
    ));
    engine.record_event("attackerDeclared", Some("player-1".into()), None, json!({}));
    assert!(
        !holds(&engine),
        "an opponent's attacker is not the controller's attack"
    );
    engine.record_event("attackerDeclared", Some("player-0".into()), None, json!({}));
    assert!(holds(&engine));
}

fn opponents_of_controller() -> Value {
    json!({"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}})
}

#[test]
fn event_history_player_relations_are_resolved_not_read_as_the_controller() {
    // `an opponent searched their library this turn`.
    let mut engine = engine_with_source(compare(
        ">=",
        json!({"kind":"countEventsThisTurn","event":"librarySearched",
            "player":opponents_of_controller(),"where":null}),
        1,
    ));
    engine.record_event("librarySearched", Some("player-0".into()), None, json!({}));
    assert!(
        !holds(&engine),
        "the controller's own search is not an opponent's"
    );
    engine.record_event("librarySearched", Some("player-1".into()), None, json!({}));
    assert!(holds(&engine));
}

#[test]
fn event_history_filters_never_fall_through_to_every_event() {
    // A filter the recorded type line cannot answer reads the event's card;
    // with no card, the count is unevaluable and the condition is false.
    let mut engine = engine_with_source(compare(
        ">=",
        json!({"kind":"countEventsThisTurn","event":"permanentDied",
            "where":{"kind":"isToken"}}),
        1,
    ));
    engine.record_event(
        "permanentDied",
        Some("player-1".into()),
        Some("vanished-object".into()),
        json!({"typeLine":"Creature - Elf"}),
    );
    assert!(!holds(&engine), "an unevaluable filter never matches");
    // A battlefield-state atom is refused at the executable boundary.
    assert!(!condition_supported(&compare(
        ">=",
        json!({"kind":"countEventsThisTurn","event":"permanentDied",
            "where":{"kind":"isTapped"}}),
        1,
    )));
    // Unknown measures and aggregates are refused, not defaulted.
    assert!(!condition_supported(&compare(
        ">=",
        json!({"kind":"countEventsThisTurn","event":"lifeLost","measure":"median"}),
        1,
    )));
}

#[test]
fn event_history_measures_sum_recorded_amounts_per_player_relation() {
    // `an opponent was dealt damage this turn`: damage to a creature has no
    // event player and never counts for a player relation.
    let mut engine = engine_with_source(compare(
        ">=",
        json!({"kind":"countEventsThisTurn","event":"damageDealt",
            "player":opponents_of_controller(),"where":null,"measure":"amount"}),
        1,
    ));
    engine.record_event(
        "damageDealt",
        None,
        Some("some-creature".into()),
        json!({"amount":3,"combat":true}),
    );
    engine.record_event(
        "damageDealt",
        Some("player-0".into()),
        None,
        json!({"amount":2,"combat":false}),
    );
    assert!(
        !holds(&engine),
        "creature damage and the controller's damage are not an opponent's"
    );
    engine.record_event(
        "damageDealt",
        Some("player-1".into()),
        None,
        json!({"amount":1,"combat":false}),
    );
    assert!(holds(&engine));
}
