//! Runtime proof of the card zone-history atom (`cards in your graveyard that
//! were put there from the battlefield this turn`, Second Sunrise and Faith's
//! Reward): only a card whose latest arrival of this turn was its departure
//! from the battlefield matches.

use super::super::*;
use serde_json::json;

fn definition(id: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: "Creature \u{2014} Bear".to_string(),
        mana_cost: "{1}{G}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
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
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        4021,
    )
    .unwrap()
}

fn record(engine: &mut GameEngine, kind: &str, id: &str) {
    engine.record_event(kind, None, Some(id.to_string()), json!({}));
}

fn record_departure(engine: &mut GameEngine, id: &str) {
    engine.record_event(
        "permanentToGraveyard",
        None,
        Some(id.to_string()),
        json!({}),
    );
    engine.record_event(
        "permanentLeftBattlefield",
        None,
        Some(id.to_string()),
        json!({ "destination": "graveyard" }),
    );
}

#[test]
fn only_cards_whose_latest_arrival_this_turn_left_the_battlefield_match() {
    let mut engine = engine();
    // A creature that died last turn, then one that dies this turn.
    record_departure(&mut engine, "died-last-turn");
    engine.state.turn_number += 1;
    record_departure(&mut engine, "died-this-turn");
    // A card discarded this turn never left the battlefield.
    record(&mut engine, "cardDiscarded", "discarded");
    // A creature that died, returned to its owner's hand and was discarded.
    record_departure(&mut engine, "died-then-discarded");
    record(&mut engine, "cardDiscarded", "died-then-discarded");

    assert!(engine.card_put_there_from_battlefield_this_turn("died-this-turn"));
    assert!(!engine.card_put_there_from_battlefield_this_turn("died-last-turn"));
    assert!(!engine.card_put_there_from_battlefield_this_turn("discarded"));
    assert!(!engine.card_put_there_from_battlefield_this_turn("died-then-discarded"));
    assert!(!engine.card_put_there_from_battlefield_this_turn("never-moved"));
}

fn record_batched_arrival(engine: &mut GameEngine, id: &str, from: Option<&str>) {
    let mut card = engine.state.players[0].library[0].clone();
    card.instance_id = id.to_string();
    engine.record_event(
        "cardsEnteredGraveyard",
        Some("player-0".to_string()),
        None,
        event_history::graveyard_arrival_detail_from(&[card], from),
    );
}

#[test]
fn batched_graveyard_arrival_after_a_departure_is_classified_by_its_origin() {
    let mut engine = engine();
    // The engine records the batched arrival after the departure records.
    record_departure(&mut engine, "died");
    record_batched_arrival(&mut engine, "died", Some("battlefield"));
    // A milled card's batched arrival comes from its library.
    record(&mut engine, "cardMilled", "milled");
    record_batched_arrival(&mut engine, "milled", Some("library"));
    // A card that died, returned to hand, then reached the graveyard by a
    // move whose origin the recorder did not know is not a departure.
    record_departure(&mut engine, "died-then-unknown");
    record_batched_arrival(&mut engine, "died-then-unknown", Some("battlefield"));
    record_batched_arrival(&mut engine, "died-then-unknown", None);

    assert!(engine.card_put_there_from_battlefield_this_turn("died"));
    assert!(!engine.card_put_there_from_battlefield_this_turn("milled"));
    assert!(!engine.card_put_there_from_battlefield_this_turn("died-then-unknown"));
}

#[test]
fn binding_replaces_history_leaves_and_keeps_other_filters_borrowed() {
    let mut engine = engine();
    record_departure(&mut engine, "returned");
    let filter = json!({ "kind": "and", "operands": [
        { "kind": "cardTypeContains", "value": "Creature" },
        { "kind": "putThereFromBattlefieldThisTurn" },
    ]});
    let bound = engine.bind_card_history_leaves("returned", &filter);
    assert_eq!(
        bound.as_ref()["operands"][1],
        json!({ "kind": "and", "operands": [] })
    );
    let negated =
        json!({ "kind": "not", "operand": { "kind": "putThereFromBattlefieldThisTurn" } });
    let bound = engine.bind_card_history_leaves("other", &negated);
    assert_eq!(
        bound.as_ref()["operand"],
        json!({ "kind": "or", "operands": [] })
    );
    let plain = json!({ "kind": "cardTypeContains", "value": "Creature" });
    assert!(matches!(
        engine.bind_card_history_leaves("returned", &plain),
        std::borrow::Cow::Borrowed(_)
    ));
}
