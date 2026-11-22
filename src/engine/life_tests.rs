use super::super::*;
use super::direct_life_effect_supported;

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
        73,
    )
    .expect("life primitive test setup is valid")
}

fn stack_object() -> StackObject {
    StackObject {
        id: "stack:life-primitive".to_string(),
        controller: "player-0".to_string(),
        card: card_instance("player-0", &definition("life-source", "Instant"), 0),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetPlayer".to_string(),
            TargetRef::Player {
                player_id: "player-1".to_string(),
            },
        )]),
    }
}

#[test]
fn validates_only_direct_life_total_primitives() {
    for effect in [
        json!({
            "kind": "gainLife",
            "player": { "kind": "abilityController" },
            "amount": { "kind": "integer", "value": 3 },
        }),
        json!({
            "kind": "loseLife",
            "player": { "kind": "chosenTarget", "id": "targetPlayer" },
            "amount": 4,
        }),
    ] {
        assert_eq!(direct_life_effect_supported(&effect), Some(true));
    }
    assert_eq!(
        direct_life_effect_supported(&json!({
            "kind": "gainLife",
            "player": { "kind": "abilityController" },
            "amount": "three",
        })),
        Some(false)
    );
    assert_eq!(
        direct_life_effect_supported(&json!({ "kind": "drainEachOpponent" })),
        None
    );
}

#[test]
fn executes_gain_and_loss_against_resolved_players() {
    let mut engine = engine();
    let stack_object = stack_object();
    let mut bindings = BTreeMap::new();
    let decisions = BTreeMap::new();

    assert!(
        engine
            .execute_direct_life_effect(
                &json!({
                    "kind": "gainLife",
                    "player": { "kind": "abilityController" },
                    "amount": 3,
                }),
                &stack_object,
                &mut bindings,
                &decisions,
            )
            .expect("gain life executes")
    );
    assert!(
        engine
            .execute_direct_life_effect(
                &json!({
                    "kind": "loseLife",
                    "player": { "kind": "chosenTarget", "id": "targetPlayer" },
                    "amount": 4,
                }),
                &stack_object,
                &mut bindings,
                &decisions,
            )
            .expect("lose life executes")
    );

    assert_eq!(engine.state.players[0].life, 23);
    assert_eq!(engine.state.players[1].life, 16);
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "lifeLost"
            && event.player_id.as_deref() == Some("player-1")
            && event.detail["amount"] == 4
    }));
    assert!(
        !engine
            .execute_direct_life_effect(
                &json!({ "kind": "drawCards" }),
                &stack_object,
                &mut bindings,
                &decisions,
            )
            .expect("unowned effects are declined")
    );
}
