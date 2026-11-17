use super::super::*;
use super::ability_signal_effect_supported;

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains");
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 7,
            starting_player: 0,
        },
        79,
    )
    .expect("ability signal test setup is valid")
}

fn stack_object() -> StackObject {
    StackObject {
        id: "stack:ability-signal".to_string(),
        controller: "player-0".to_string(),
        card: card_instance("player-0", &definition("signal-source", "Instant"), 0),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

#[test]
fn validates_only_well_formed_generic_ability_signals() {
    assert_eq!(
        ability_signal_effect_supported(&json!({
            "kind": "emitAbilitySignal",
            "abilityId": "surveil",
            "player": { "kind": "abilityController" },
        })),
        Some(true)
    );
    assert_eq!(
        ability_signal_effect_supported(&json!({
            "kind": "emitAbilitySignal",
            "abilityId": "",
            "player": { "kind": "abilityController" },
        })),
        Some(false)
    );
    assert_eq!(
        ability_signal_effect_supported(&json!({ "kind": "drawCards" })),
        None
    );
}

#[test]
fn publishes_one_typed_event_and_enqueues_matching_triggers() {
    let mut engine = engine();
    let mut watcher_definition = definition("surveil-watcher", "Creature - Wizard");
    watcher_definition.rules.push(json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "abilityPerformed",
            "abilityId": "surveil",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
        "effects": [{
            "kind": "drawCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": { "kind": "integer", "value": 1 },
        }],
    }));
    engine.state.players[0]
        .battlefield
        .push(card_instance("player-0", &watcher_definition, 1));

    assert!(
        engine
            .execute_ability_signal_effect(
                &json!({
                    "kind": "emitAbilitySignal",
                    "abilityId": "surveil",
                    "player": { "kind": "abilityController" },
                    "payload": { "inspected": 2 },
                }),
                &stack_object(),
                &BTreeMap::new(),
            )
            .expect("ability signal executes")
    );

    assert!(engine.state.events.iter().any(|event| {
        event.kind == "abilityPerformed"
            && event.player_id.as_deref() == Some("player-0")
            && event.detail["abilityId"] == "surveil"
            && event.detail["payload"]["inspected"] == 2
    }));
    assert_eq!(engine.state.stack.len(), 1);
}
