//! Runtime proofs of action-result objects (CR 608.2c, 608.2h): the one
//! object a producing action bound is read live, as last known information
//! once that action removed it from the battlefield, or as the printed card
//! it is outside the battlefield; an action that bound nothing reads 0.
use super::super::*;

fn definition(id: &str, type_line: &str, power: &str, toughness: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{3}{B}".to_string(),
        power: Some(power.to_string()),
        toughness: Some(toughness.to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
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
                    cards: vec![definition("filler", "Instant", "0", "0"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        6061,
    )
    .expect("action-result runtime test setup is valid")
}

fn toughness_of(binding: &str) -> Value {
    json!({"kind": "toughnessOf", "object": {"kind": "singleBoundObject", "binding": binding}})
}

#[test]
fn a_sacrificed_creature_reports_the_toughness_it_last_had_on_the_battlefield() {
    let mut engine = engine();
    let mut creature = card_instance(
        "player-1",
        &definition("victim", "Creature — Zombie", "2", "2"),
        0,
    );
    creature.instance_id = "victim".to_string();
    // Buffed until end of turn: +3/+3 is part of its last known toughness.
    creature.power_modifier = 3;
    creature.toughness_modifier = 3;
    engine.state.players[1].battlefield.push(creature);
    let producer = json!({
        "kind": "sacrificePermanents",
        "player": {"kind": "abilityController"},
        "where": null,
        "count": 1,
        "bind": "actionResult:0",
    });
    let snapshot = engine.bound_object_lki_snapshot(&producer);
    assert!(snapshot.is_some(), "a removing producer snapshots first");
    // The sacrifice moves it to its owner's graveyard (CR 701.21a).
    let mut sacrificed = engine.state.players[1].battlefield.remove(0);
    super::super::reset_after_leaving_battlefield(&mut sacrificed);
    engine.state.players[1].graveyard.push(sacrificed);
    let mut bindings = BTreeMap::new();
    bindings.insert(
        "actionResult:0".to_string(),
        RuntimeBinding::Objects(vec!["victim".to_string()]),
    );
    let mut decisions = BTreeMap::new();
    engine.record_bound_object_lki(&producer, snapshot, &bindings, &mut decisions);
    assert_eq!(
        engine.bound_object_numeric_read(&toughness_of("actionResult:0"), &bindings, &decisions),
        Some(Some(5)),
        "last known toughness includes the until-end-of-turn bonus",
    );
    // The card in the graveyard has its printed toughness; the read keeps
    // the last known information, not the new object's (CR 400.7).
    assert_ne!(
        engine.bound_object_numeric_read(&toughness_of("actionResult:0"), &bindings, &decisions),
        Some(Some(2)),
    );
}

#[test]
fn a_card_result_outside_the_battlefield_reads_its_printed_values() {
    let mut engine = engine();
    let mut card = card_instance(
        "player-0",
        &definition("discarded", "Creature — Elf", "4", "1"),
        0,
    );
    card.instance_id = "discarded".to_string();
    engine.state.players[0].graveyard.push(card);
    let mut bindings = BTreeMap::new();
    bindings.insert(
        "discardResult".to_string(),
        RuntimeBinding::Objects(vec!["discarded".to_string()]),
    );
    let decisions = BTreeMap::new();
    let power = json!({"kind": "powerOf",
        "object": {"kind": "singleBoundObject", "binding": "discardResult"}});
    let mana_value = json!({"kind": "manaValueOf",
        "object": {"kind": "singleBoundObject", "binding": "discardResult"}});
    assert_eq!(
        engine.bound_object_numeric_read(&power, &bindings, &decisions),
        Some(Some(4))
    );
    assert_eq!(
        engine.bound_object_numeric_read(&mana_value, &bindings, &decisions),
        Some(Some(4))
    );
}

#[test]
fn an_empty_result_reads_zero_and_live_or_unbound_objects_are_not_owned() {
    let mut engine = engine();
    let mut bindings = BTreeMap::new();
    bindings.insert("nothing".to_string(), RuntimeBinding::Objects(Vec::new()));
    let decisions = BTreeMap::new();
    // No creature was sacrificed: the life gained is 0, not an error.
    assert_eq!(
        engine.bound_object_numeric_read(&toughness_of("nothing"), &bindings, &decisions),
        Some(Some(0))
    );
    // A live permanent keeps the existing live reader.
    let mut creature = card_instance(
        "player-0",
        &definition("alive", "Creature — Bear", "2", "2"),
        0,
    );
    creature.instance_id = "alive".to_string();
    engine.state.players[0].battlefield.push(creature);
    bindings.insert(
        "live".to_string(),
        RuntimeBinding::Objects(vec!["alive".to_string()]),
    );
    assert_eq!(
        engine.bound_object_numeric_read(&toughness_of("live"), &bindings, &decisions),
        None
    );
    // An unbound name (a captured delayed binding) is not this reader's.
    assert_eq!(
        engine.bound_object_numeric_read(&toughness_of("captured"), &bindings, &decisions),
        None
    );
    // A producer that removes nothing it binds takes no snapshot.
    assert!(
        engine
            .bound_object_lki_snapshot(&json!({"kind": "mill", "bind": "milled"}))
            .is_none()
    );
}
