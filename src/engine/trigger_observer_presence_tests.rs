use super::*;
use serde_json::json;

fn definition(id: &str, type_line: &str) -> CardDefinition {
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
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1893,
    )
    .unwrap()
}
fn add(
    engine: &mut GameEngine,
    owner: usize,
    controller: usize,
    id: &str,
    type_line: &str,
) -> CardInstance {
    let mut card = card_instance(
        &format!("player-{owner}"),
        &definition(id, type_line),
        engine.state.players[controller].battlefield.len() + 100,
    );
    card.controller = format!("player-{controller}");
    engine.state.players[controller]
        .battlefield
        .push(card.clone());
    card
}
fn observer_rule() -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"permanentSacrificed","where":{"kind":"isToken"},
                 "player":{"kind":"controllerOf","object":{"kind":"self"}}},
        "effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},
                    "amount":{"kind":"integer","value":1}}]})
}

#[test]
fn frozen_departure_observer_remains_valid_after_source_has_left() {
    let mut e = engine();
    let watcher = add(&mut e, 0, 0, "watcher", "Creature");
    e.state.players[0].battlefield[0].definition.rules = vec![observer_rule()];
    let captured = e.capture_battlefield_departure_trigger_sources();
    assert_eq!(captured.len(), 1);
    e.state.players[0].battlefield.clear();
    let mut token = card_instance("player-0", &definition("token", "Artifact"), 901);
    token.definition.is_token = true;
    e.enqueue_permanent_sacrificed_triggers_from_sources(&token, &captured);
    assert_eq!(e.state.stack.len(), 1);
    assert_eq!(e.state.stack[0].card.instance_id, watcher.instance_id);
}

#[test]
fn admission_preserves_room_and_class_activity_and_does_not_require_live_position() {
    let e = engine();
    let mut snapshot = card_instance("player-0", &definition("departed", "Creature"), 909);
    let mut rule = observer_rule();
    rule["roomDoorIndex"] = json!(0);
    rule["minimumClassLevel"] = json!(2);
    assert!(!e.battlefield_observer_rule_active(&snapshot, &rule));
    snapshot.flags.insert("roomDoor0Unlocked".to_string(), true);
    assert!(!e.battlefield_observer_rule_active(&snapshot, &rule));
    snapshot.counters.insert("classLevel".to_string(), 2);
    assert!(e.battlefield_observer_rule_active(&snapshot, &rule));
    snapshot.flags.insert("phasedOut".to_string(), true);
    assert!(!e.battlefield_observer_rule_active(&snapshot, &rule));
}
