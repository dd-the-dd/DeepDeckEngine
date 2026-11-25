use super::super::{CardDefinition, GameSetup, PlayerDeck, card_instance};
use super::*;
use serde_json::json;

fn definition(id: &str, type_line: &str, mana_cost: &str, rules: Value) -> CardDefinition {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "typeLine": type_line,
        "manaCost": mana_cost,
        "rules": rules,
    }))
    .unwrap()
}

fn static_rule(modifier: Value) -> Value {
    json!([{
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [modifier],
    }])
}

/// p0 controls `source` (with the prohibition) and a vanilla creature.
fn engine_with(modifier: Value) -> GameEngine {
    let land = definition("land", "Land", "", json!([]));
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![land.clone(); 4],
                })
                .collect(),
        },
        4242,
    )
    .unwrap();
    let source = definition("source", "Enchantment", "{2}", static_rule(modifier));
    let bear = definition("bear", "Creature — Bear", "{1}{G}", json!([]));
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &source, 60));
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &bear, 61));
    engine
}

fn creatures_you_control() -> Value {
    json!({
        "kind": "permanents",
        "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
        "where": { "kind": "cardTypeContains", "value": "Creature" },
    })
}

/// `can't be the targets of white spells your opponents control or
/// abilities from white sources`: each branch keeps its own kind, source
/// characteristics and controller; anything else may still target.
#[test]
fn targeting_prohibition_matches_only_its_branches() {
    let modifier = json!({
        "kind": "cantBeTargeted",
        "objects": creatures_you_control(),
        "sources": [
            {
                "stackObject": "spell",
                "where": { "kind": "colorContains", "value": "white" },
                "controller": { "kind": "opponentsOf", "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            },
            {
                "stackObject": "ability",
                "where": { "kind": "colorContains", "value": "white" },
            },
        ],
    });
    assert_eq!(static_object_prohibition_supported(&modifier), Some(true));
    let engine = engine_with(modifier);
    let bear = &engine.state.players[0].battlefield[1];
    let white = definition("white", "Instant", "{W}", json!([]));
    let blue = definition("blue", "Instant", "{U}", json!([]));
    // An opponent's white spell, and a white source's ability of anyone.
    assert!(engine.static_targeting_prohibited(bear, "p1", &white, true));
    assert!(engine.static_targeting_prohibited(bear, "p0", &white, false));
    // The controller's own white spell and any blue source may target.
    assert!(!engine.static_targeting_prohibited(bear, "p0", &white, true));
    assert!(!engine.static_targeting_prohibited(bear, "p1", &blue, true));
    assert!(!engine.static_targeting_prohibited(bear, "p1", &blue, false));
    // The prohibition protects only its own objects.
    let source = &engine.state.players[0].battlefield[0];
    assert!(!engine.static_targeting_prohibited(source, "p1", &white, true));
}

/// A branch with an unknown stack-object kind is refused, never ignored.
#[test]
fn targeting_prohibition_refuses_an_unknown_branch() {
    let modifier = json!({
        "kind": "cantBeTargeted",
        "objects": { "kind": "self" },
        "sources": [{ "stackObject": "emblem" }],
    });
    assert_eq!(static_object_prohibition_supported(&modifier), Some(false));
}

/// `can't become untapped` stops every untap attempt; without it the same
/// attempt untaps.
#[test]
fn untap_prohibition_keeps_the_object_tapped() {
    let mut engine = engine_with(json!({
        "kind": "cantBecomeUntapped",
        "objects": creatures_you_control(),
    }));
    engine.state.players[0].battlefield[1].tapped = true;
    let id = engine.state.players[0].battlefield[1].instance_id.clone();
    assert_eq!(
        engine.attempt_untap_permanent(&id),
        super::super::untap_attempt::UntapAttemptOutcome::Unchanged
    );
    assert!(engine.state.players[0].battlefield[1].tapped);
    engine.state.players[0].battlefield.remove(0);
    assert_eq!(
        engine.attempt_untap_permanent(&id),
        super::super::untap_attempt::UntapAttemptOutcome::Untapped
    );
}

/// `can't have +1/+1 counters put on them` forbids only that kind.
#[test]
fn typed_counter_prohibition_forbids_only_its_kind() {
    let engine = engine_with(json!({
        "kind": "preventCounters",
        "objects": creatures_you_control(),
        "counter": "+1/+1",
    }));
    let bear = &engine.state.players[0].battlefield[1];
    assert!(engine.static_counter_prohibited(bear, "+1/+1"));
    assert!(!engine.static_counter_prohibited(bear, "-1/-1"));
}
