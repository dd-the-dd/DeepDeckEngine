//! Runtime proofs of event antecedent producers: a `one or more` attack
//! batch binds the number of attackers its pattern matched (`draw that many
//! cards`, CR 508.1, 603.2c), counting only the source controller's
//! attackers that satisfy the pattern's predicate.
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
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        6032,
    )
    .expect("antecedent producer test setup is valid")
}

fn put(engine: &mut GameEngine, player: usize, id: &str, type_line: &str) -> CardInstance {
    let mut card = card_instance(&format!("player-{player}"), &definition(id, type_line), 0);
    card.instance_id = id.to_string();
    engine.state.players[player].battlefield.push(card.clone());
    card
}

#[test]
fn an_attack_batch_binds_the_count_of_matching_controlled_attackers() {
    let mut engine = engine();
    let source = put(
        &mut engine,
        0,
        "ur-dragon",
        "Legendary Creature — Dragon Avatar",
    );
    put(&mut engine, 0, "dragon-a", "Creature — Dragon");
    put(&mut engine, 0, "dragon-b", "Creature — Dragon");
    put(&mut engine, 0, "goblin", "Creature — Goblin");
    put(&mut engine, 1, "their-dragon", "Creature — Dragon");
    let rule = json!({"kind":"triggeredAbility",
        "event":{"kind":"controlledCreaturesAttacked",
            "player":{"kind":"controllerOf","object":{"kind":"self"}},
            "where":{"kind":"subtypeContains","value":"Dragon"}},
        "effects":[]});
    let attackers = ["dragon-a", "dragon-b", "goblin", "their-dragon"].map(str::to_string);
    let mut decisions = BTreeMap::new();
    engine.bind_attack_batch_count(&rule, &source, &attackers, &mut decisions);
    assert_eq!(decisions["eventObjectCount"], json!(2));

    // A rule without the batch pattern binds no count.
    let mut decisions = BTreeMap::new();
    let other = json!({"kind":"triggeredAbility",
        "event":{"kind":"declaredAttacker"},"effects":[]});
    engine.bind_attack_batch_count(&other, &source, &attackers, &mut decisions);
    assert!(!decisions.contains_key("eventObjectCount"));
}
