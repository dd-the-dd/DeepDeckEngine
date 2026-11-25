//! Runtime proofs of the player keyword-action matchers: a crime (CR
//! 700.13), die results (CR 706.2), coin flips (CR 705.2), clashes (CR
//! 701.30d), the per-player attack batch with its attacked player (CR
//! 508.1, 603.2c), the opponent's mana-ability observer (CR 106.11a) and
//! the state trigger's single pending occurrence (CR 603.8).
use super::*;
use serde_json::json;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules,
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", Vec::new()); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        700,
    )
    .expect("player event matcher test setup is valid")
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn opponents() -> Value {
    json!({ "kind": "opponentsOf", "player": controller() })
}

fn draw_rule(event: Value) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": event,
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": { "kind": "integer", "value": 1 },
        }],
    })
}

fn add(
    engine: &mut GameEngine,
    player: usize,
    id: &str,
    type_line: &str,
    rules: Vec<Value>,
) -> CardInstance {
    let mut card = card_instance(
        &format!("player-{player}"),
        &definition(id, type_line, rules),
        0,
    );
    card.instance_id = id.to_string();
    card.printed_definition = Some(card.definition.clone());
    engine.state.players[player].battlefield.push(card.clone());
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    card
}

fn triggered(engine: &GameEngine) -> Vec<&StackObject> {
    engine
        .state
        .stack
        .iter()
        .filter(|object| object.ability_kind.as_deref() == Some("triggeredAbility"))
        .collect()
}

/// A spell of `player` on the stack with the given targets.
fn targeting_spell(
    engine: &mut GameEngine,
    player: usize,
    stack_id: &str,
    targets: &BTreeMap<String, TargetRef>,
) {
    let card = card_instance(
        &format!("player-{player}"),
        &definition(stack_id, "Instant", Vec::new()),
        0,
    );
    engine.state.stack.push(StackObject {
        id: stack_id.to_string(),
        controller: format!("player-{player}"),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: targets.clone(),
    });
}

#[test]
fn emitted_player_event_shapes_are_executable_and_their_near_misses_are_not() {
    for (event, executable) in [
        (
            json!({"kind": "crimeCommitted", "player": controller()}),
            true,
        ),
        (
            json!({"kind": "crimeCommitted", "player": opponents()}),
            true,
        ),
        (
            json!({"kind": "crimeCommitted", "player": controller(), "where": {}}),
            false,
        ),
        (
            json!({"kind": "dieRolled", "player": controller(), "minimum": 5}),
            true,
        ),
        (
            json!({"kind": "dieRolled", "player": controller(), "minimum": 0}),
            false,
        ),
        (
            json!({"kind": "dieRolled", "player": controller(), "aggregation": "oneOrMore"}),
            true,
        ),
        // A batch has no single result to filter.
        (
            json!({"kind": "dieRolled", "player": controller(), "aggregation": "oneOrMore", "minimum": 6}),
            false,
        ),
        (
            json!({"kind": "coinFlipped", "player": controller(), "outcome": "won"}),
            true,
        ),
        (
            json!({"kind": "coinFlipped", "player": controller(), "outcome": "heads"}),
            false,
        ),
        (
            json!({"kind": "clashed", "player": controller(), "won": true}),
            true,
        ),
        (json!({"kind": "proliferated", "player": opponents()}), true),
        (
            json!({"kind": "permanentExerted", "player": controller()}),
            true,
        ),
        (
            json!({"kind": "lifeGained", "player": controller(), "firstTimeEachTurn": true}),
            true,
        ),
        // The history keys the occurrence by the source controller only.
        (
            json!({"kind": "lifeGained", "player": opponents(), "firstTimeEachTurn": true}),
            false,
        ),
        (
            json!({"kind": "cardDrawn", "player": controller(), "firstTimeEachTurn": true}),
            false,
        ),
        (
            json!({"kind": "attackersAttackedPlayer", "attackers": {"controller": controller()}, "attacked": opponents()}),
            true,
        ),
        (
            json!({"kind": "attackersAttackedPlayer", "attackers": {"minimum": 0}, "attacked": controller()}),
            false,
        ),
    ] {
        assert_eq!(
            triggered_rule_supported(&draw_rule(event.clone())),
            executable,
            "{event}"
        );
    }
}

#[test]
fn a_crime_is_committed_once_when_an_opponent_or_their_permanent_is_targeted() {
    let mut engine = engine();
    add(
        &mut engine,
        0,
        "crime-observer",
        "Creature",
        vec![draw_rule(
            json!({"kind": "crimeCommitted", "player": controller()}),
        )],
    );
    add(&mut engine, 0, "own-creature", "Creature", Vec::new());
    add(&mut engine, 1, "their-creature", "Creature", Vec::new());

    // Only the controller's own permanent: no crime.
    let own = BTreeMap::from([(
        "target".to_string(),
        TargetRef::Permanent {
            instance_id: "own-creature".to_string(),
        },
    )]);
    targeting_spell(&mut engine, 0, "stack:own", &own);
    engine.publish_committed_crime("stack:own", &own);
    assert!(triggered(&engine).is_empty());

    // An opponent and their permanent: one crime for the whole spell.
    let crime = BTreeMap::from([
        (
            "player".to_string(),
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        ),
        (
            "creature".to_string(),
            TargetRef::Permanent {
                instance_id: "their-creature".to_string(),
            },
        ),
    ]);
    targeting_spell(&mut engine, 0, "stack:crime", &crime);
    engine.publish_committed_crime("stack:crime", &crime);
    let fired = triggered(&engine);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].decisions["triggeringPlayerId"], json!("player-0"));

    // The opponent's own crime is not the observer's (`you commit`).
    let their_crime = BTreeMap::from([(
        "target".to_string(),
        TargetRef::Permanent {
            instance_id: "own-creature".to_string(),
        },
    )]);
    targeting_spell(&mut engine, 1, "stack:their", &their_crime);
    engine.publish_committed_crime("stack:their", &their_crime);
    assert_eq!(triggered(&engine).len(), 1);
}

#[test]
fn die_results_and_batches_trigger_their_own_patterns() {
    let mut engine = engine();
    add(
        &mut engine,
        0,
        "five-or-higher",
        "Enchantment",
        vec![draw_rule(
            json!({"kind": "dieRolled", "player": controller(), "minimum": 5}),
        )],
    );
    add(
        &mut engine,
        0,
        "one-or-more",
        "Enchantment",
        vec![draw_rule(
            json!({"kind": "dieRolled", "player": controller(), "aggregation": "oneOrMore"}),
        )],
    );
    engine.publish_die_rolled("player-0", 3);
    assert!(triggered(&engine).is_empty());
    engine.publish_die_rolled("player-0", 6);
    engine.publish_dice_rolled_batch("player-0");
    let sources = triggered(&engine)
        .iter()
        .map(|object| object.card.instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(sources, vec!["five-or-higher", "one-or-more"]);
    // Another player's roll is not `you roll`.
    engine.publish_die_rolled("player-1", 6);
    assert_eq!(triggered(&engine).len(), 2);
}

#[test]
fn flips_and_clashes_match_their_outcome() {
    let mut engine = engine();
    add(
        &mut engine,
        0,
        "flip-won",
        "Enchantment",
        vec![draw_rule(
            json!({"kind": "coinFlipped", "player": controller(), "outcome": "won"}),
        )],
    );
    add(
        &mut engine,
        0,
        "clash-won",
        "Enchantment",
        vec![draw_rule(
            json!({"kind": "clashed", "player": controller(), "won": true}),
        )],
    );
    engine.publish_coin_flipped("player-0", false);
    engine.publish_clashed(
        &["player-0".to_string(), "player-1".to_string()],
        Some("player-1"),
    );
    assert!(triggered(&engine).is_empty());
    engine.publish_coin_flipped("player-0", true);
    engine.publish_clashed(
        &["player-0".to_string(), "player-1".to_string()],
        Some("player-0"),
    );
    assert_eq!(triggered(&engine).len(), 2);
}

#[test]
fn one_attack_batch_triggers_once_per_attacked_player_with_that_player_bound() {
    let mut engine = engine();
    add(
        &mut engine,
        0,
        "batch-observer",
        "Creature",
        vec![draw_rule(json!({
            "kind": "attackersAttackedPlayer",
            "attackers": {"controller": controller()},
            "attacked": opponents(),
        }))],
    );
    add(&mut engine, 0, "attacker-a", "Creature", Vec::new());
    add(&mut engine, 0, "attacker-b", "Creature", Vec::new());
    engine.state.combat.attackers = ["attacker-a", "attacker-b"]
        .into_iter()
        .map(|attacker| DeclaredAttacker {
            attacker_id: attacker.to_string(),
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        })
        .collect();
    engine.publish_attackers_attacked_player(&["attacker-a".to_string(), "attacker-b".to_string()]);
    let fired = triggered(&engine);
    assert_eq!(fired.len(), 1, "one event for one attacked player");
    assert_eq!(fired[0].decisions["triggeringPlayerId"], json!("player-1"));
    assert_eq!(fired[0].decisions["eventObjectCount"], json!(2));
}

#[test]
fn an_opponents_mana_ability_is_observed_through_the_written_relation() {
    let mut engine = engine();
    let observer = add(&mut engine, 0, "observer", "Enchantment", Vec::new());
    let own_land = add(&mut engine, 0, "own-land", "Land", Vec::new());
    let their_land = add(&mut engine, 1, "their-land", "Land", Vec::new());
    let event = json!({
        "kind": "controlledPermanentManaAbilityActivated",
        "player": opponents(),
        "where": {"kind": "cardTypeContains", "value": "Land"},
    });
    assert!(engine.tapped_for_mana_observed(&event, &observer, &their_land));
    assert!(!engine.tapped_for_mana_observed(&event, &observer, &own_land));
}

#[test]
fn a_state_trigger_fires_once_and_waits_while_it_is_pending() {
    let mut engine = engine();
    add(
        &mut engine,
        0,
        "state-source",
        "Creature",
        vec![draw_rule(json!({
            "kind": "stateConditionMet",
            "condition": {
                "kind": "compare",
                "left": {"kind": "countCounters", "object": {"kind": "self"}, "counter": "charge"},
                "operator": "==",
                "right": {"kind": "integer", "value": 0},
            },
        }))],
    );
    engine.enqueue_state_triggers();
    engine.enqueue_state_triggers();
    assert_eq!(
        triggered(&engine).len(),
        1,
        "CR 603.8: one pending occurrence"
    );
}
