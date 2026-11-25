//! Runtime proofs of the object-query relations and atoms added with the
//! batch 9 object-query grammar:
//!
//! - `that entered the battlefield under your control this turn` (Shaile,
//!   Hobgoblin Bandit Lord): the entry controller is the player who
//!   controlled the permanent as it entered, read from the entry record of
//!   this incarnation, not its current controller (CR 110.2a);
//! - `that was put there this turn` / `that was milled this turn` (Abyssal
//!   Harvester, Tato Farmer): the latest zone arrival of the card this turn
//!   (CR 400.7, 701.13b);
//! - `two target creature cards that share a creature type` (Raise the
//!   Draugr): a constraint over the chosen set, rechecked against the
//!   objects as they are when the targets are checked again (CR 608.2b);
//! - `with toughness greater than its power` (Bedrock Tortoise): two
//!   properties of the same candidate.

use super::super::*;
use serde_json::{Value, json};

fn definition(id: &str, type_line: &str, power: &str, toughness: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{1}{G}".to_string(),
        power: Some(power.to_string()),
        toughness: Some(toughness.to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn bear(id: &str) -> CardDefinition {
    definition(id, "Creature \u{2014} Bear", "2", "2")
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![bear("filler"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        4021,
    )
    .unwrap()
}

/// A card instance of `definition` owned by `player`, with a fresh id.
fn instance(
    engine: &GameEngine,
    player: usize,
    id: &str,
    definition: CardDefinition,
) -> CardInstance {
    let mut card = engine.state.players[player].library[0].clone();
    card.instance_id = id.to_string();
    card.definition = definition;
    card.printed_definition = None;
    card.flags.clear();
    card.counters.clear();
    card
}

fn record(engine: &mut GameEngine, kind: &str, player: Option<&str>, id: &str, detail: Value) {
    engine.record_event(
        kind,
        player.map(str::to_string),
        Some(id.to_string()),
        detail,
    );
}

#[test]
fn entry_controller_is_the_controller_as_the_permanent_entered() {
    let mut engine = engine();
    let mut stolen = instance(&engine, 0, "stolen", bear("stolen"));
    stolen.flags.insert("enteredThisTurn".to_string(), true);
    record(
        &mut engine,
        "permanentEnteredBattlefield",
        Some("player-0"),
        "stolen",
        json!({}),
    );
    // Control changes after the entry (CR 110.2a): the entry fact stays.
    stolen.controller = "player-1".to_string();
    engine.state.players[1].battlefield.push(stolen);

    let mut old = instance(&engine, 0, "old", bear("old"));
    old.controller = "player-0".to_string();
    engine.state.players[0].battlefield.push(old);

    let unprepared = json!({
        "kind": "enteredUnderControlThisTurn",
        "player": "abilityController",
    });
    assert!(object_relation_eval::relation_atom_supported(&unprepared));
    assert!(!object_relation_eval::relation_atom_supported(&json!({
        "kind": "enteredUnderControlThisTurn",
        "player": "anyPlayer",
    })));

    let yours = json!({
        "kind": "enteredUnderControlThisTurn",
        "player": "abilityController",
        "playerIds": ["player-0"],
        "anchorIds": [],
    });
    let theirs = json!({
        "kind": "enteredUnderControlThisTurn",
        "player": "abilityController",
        "playerIds": ["player-1"],
        "anchorIds": [],
    });
    let stolen = engine.state.players[1].battlefield.last().unwrap().clone();
    let old = engine.state.players[0].battlefield.last().unwrap().clone();
    assert!(engine.eval_object_relation(
        &yours,
        runtime_object_filter::ObjectFilterView::Permanent(&stolen)
    ));
    assert!(!engine.eval_object_relation(
        &theirs,
        runtime_object_filter::ObjectFilterView::Permanent(&stolen)
    ));
    // A permanent that did not enter this turn never matches.
    assert!(!engine.eval_object_relation(
        &yours,
        runtime_object_filter::ObjectFilterView::Permanent(&old)
    ));
    // An unprepared atom (no player ids) never matches.
    assert!(!engine.eval_object_relation(
        &unprepared,
        runtime_object_filter::ObjectFilterView::Permanent(&stolen)
    ));
}

#[test]
fn zone_arrival_histories_read_the_latest_arrival_of_this_turn() {
    let mut engine = engine();
    record(
        &mut engine,
        "cardMilled",
        None,
        "milled-last-turn",
        json!({}),
    );
    engine.state.turn_number += 1;
    record(&mut engine, "cardMilled", None, "milled", json!({}));
    record(&mut engine, "cardDiscarded", None, "discarded", json!({}));
    // Milled, returned to hand, then discarded: no longer a milled card.
    record(
        &mut engine,
        "cardMilled",
        None,
        "milled-then-discarded",
        json!({}),
    );
    record(
        &mut engine,
        "cardDiscarded",
        None,
        "milled-then-discarded",
        json!({}),
    );

    let milled = object_event_history::MILLED_THIS_TURN;
    let put_there = object_event_history::PUT_THERE_THIS_TURN;
    assert_eq!(engine.card_history_matches("milled", milled), Some(true));
    assert_eq!(engine.card_history_matches("milled", put_there), Some(true));
    assert_eq!(
        engine.card_history_matches("discarded", milled),
        Some(false)
    );
    assert_eq!(
        engine.card_history_matches("discarded", put_there),
        Some(true)
    );
    assert_eq!(
        engine.card_history_matches("milled-then-discarded", milled),
        Some(false)
    );
    assert_eq!(
        engine.card_history_matches("milled-last-turn", put_there),
        Some(false)
    );
    assert_eq!(engine.card_history_matches("milled", "isTapped"), None);

    // The binder replaces the leaves by constants for one instance.
    let filter = json!({ "kind": "and", "operands": [
        { "kind": "cardTypeContains", "value": "Creature" },
        { "kind": "milledThisTurn" },
    ]});
    let bound = engine.bind_card_history_leaves("discarded", &filter);
    assert_eq!(
        bound.as_ref()["operands"][1],
        json!({ "kind": "or", "operands": [] })
    );
}

#[test]
fn a_chosen_set_shares_a_creature_type_while_each_member_exists() {
    let mut engine = engine();
    let elf = definition("elf", "Creature \u{2014} Elf Warrior", "1", "1");
    let warrior = definition("warrior", "Creature \u{2014} Human Warrior", "2", "1");
    let changeling = definition("shapeshifter", "Creature \u{2014} Shapeshifter", "1", "1");
    for (id, definition) in [
        ("elf", elf),
        ("warrior", warrior),
        ("bear", bear("bear")),
        ("shapeshifter", changeling),
    ] {
        let card = instance(&engine, 0, id, definition);
        engine.state.players[0].graveyard.push(card);
    }
    let shared = |engine: &GameEngine, ids: &[&str]| {
        let ids = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        engine.selection_satisfies(
            &ids,
            &json!([{ "kind": "sharedValue", "characteristic": "creatureType" }]),
        )
    };
    assert!(object_predicate_eval::set_constraint_supported(&json!({
        "kind": "sharedValue",
        "characteristic": "creatureType",
    })));
    assert!(!object_predicate_eval::set_constraint_supported(&json!({
        "kind": "sharedValue",
        "characteristic": "artist",
    })));
    assert!(shared(&engine, &["elf", "warrior"]));
    assert!(!shared(&engine, &["elf", "bear"]));

    // CR 608.2b: when the targets are checked again, a member that left
    // the graveyard no longer exists there and the set fails.
    let targets = [
        TargetRef::Card {
            instance_id: "elf".to_string(),
        },
        TargetRef::Card {
            instance_id: "warrior".to_string(),
        },
    ];
    let constraint = json!({ "kind": "sharedValue", "characteristic": "creatureType" });
    assert!(engine.target_selection_satisfies_set_constraint(&targets, &constraint));
    engine.state.players[0]
        .graveyard
        .retain(|card| card.instance_id != "warrior");
    assert!(!engine.target_selection_satisfies_set_constraint(&targets, &constraint));
}

#[test]
fn a_candidate_property_compares_with_another_property_of_the_same_candidate() {
    let tortoise_like = definition("wall", "Creature \u{2014} Wall", "0", "4");
    let striker = definition("striker", "Creature \u{2014} Human", "4", "1");
    let filter = json!({
        "kind": "compare",
        "operator": ">",
        "left": { "kind": "toughnessOf", "object": { "kind": "candidate" } },
        "right": { "kind": "powerOf", "object": { "kind": "candidate" } },
    });
    assert!(card_filter_supported(&filter));
    assert!(object_predicate::matches_object_atom(
        object_predicate::DefinitionView::new(&tortoise_like),
        &filter,
    ));
    assert!(!object_predicate::matches_object_atom(
        object_predicate::DefinitionView::new(&striker),
        &filter,
    ));
}
