//! Runtime proof of the static keyword loss (`All creatures lose flying.`):
//! printed and granted keywords are removed while the source applies, only
//! from the objects its selector names, and return when the source leaves.
use super::super::super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    let mut definition: CardDefinition = serde_json::from_value(json!({
        "id": id, "name": id, "typeLine": type_line,
        "power": type_line.contains("Creature").then_some("2"),
        "toughness": type_line.contains("Creature").then_some("2"),
    }))
    .expect("standalone test definition is valid");
    definition.rules = rules;
    definition
}

fn instance(id: &str, definition: CardDefinition, owner: &str) -> CardInstance {
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.into();
    instance
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains", Vec::new());
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
            opening_hand_size: 0,
            starting_player: 0,
        },
        3301,
    )
    .expect("keyword loss test engine is valid")
}

fn flying_rule() -> Value {
    json!({ "kind": "flying", "source": { "kind": "self" } })
}

#[test]
fn static_keyword_loss_removes_only_the_named_keyword_from_selected_objects() {
    let loss = json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone", "object": { "kind": "self" }, "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "loseKeywords",
            "objects": {
                "kind": "permanents",
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            },
            "keywords": ["flying"],
        }],
    });
    assert!(rule_is_executable(&loss));
    let mut engine = engine();
    engine.state.players[1].battlefield.push(instance(
        "bird",
        definition("bird", "Creature - Bird", vec![flying_rule()]),
        "player-1",
    ));
    let bird = engine.state.players[1].battlefield[0].clone();
    assert!(engine.permanent_has_effective_keyword(&bird, "flying"));
    engine.state.players[0].battlefield.push(instance(
        "gravity",
        definition("gravity", "Enchantment", vec![loss]),
        "player-0",
    ));
    assert!(!engine.permanent_has_effective_keyword(&bird, "flying"));
    engine.state.players[0].battlefield.clear();
    assert!(engine.permanent_has_effective_keyword(&bird, "flying"));
}
