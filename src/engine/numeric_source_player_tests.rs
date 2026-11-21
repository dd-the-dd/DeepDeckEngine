use crate::engine::*;

fn engine() -> GameEngine {
    let card: CardDefinition = serde_json::from_value(
        json!({"id":"filler","name":"Filler","typeLine":"Artifact","rules":[]}),
    )
    .unwrap();
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("p{n}"),
                    name: format!("P{n}"),
                    starting_life: if n == 0 { 18 } else { 11 },
                    cards: vec![card.clone(); 8],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        13,
    )
    .unwrap()
}

#[test]
fn numeric_life_source_relative_players_preserve_actor_owner_and_physical_controller() {
    let e = engine();
    let mut source = e.state.players[1].library[0].clone();
    source.owner = "p1".into();
    source.controller = "p1".into();
    for (reference, expected) in [
        (json!({"kind":"abilityController"}), 18),
        (json!({"kind":"controllerOf","object":{"kind":"self"}}), 18),
        (json!({"kind":"ownerOf","object":{"kind":"self"}}), 11),
        (
            json!({"kind":"controllerOfObject","object":{"kind":"self"}}),
            11,
        ),
    ] {
        assert_eq!(
            e.numeric_expression_with_source(
                &json!({"kind":"lifeTotal","player":reference}),
                "p0",
                Some(&source),
            ),
            Some(expected)
        );
    }
}

#[test]
fn numeric_source_player_does_not_fabricate_an_owner_or_target_context() {
    let e = engine();
    for reference in [
        json!({"kind":"ownerOf","object":{"kind":"self"}}),
        json!({"kind":"controllerOfObject","object":{"kind":"self"}}),
        json!({"kind":"controllerOf","object":{"kind":"chosenObject","id":"missing"}}),
        json!({"kind":"eachPlayer"}),
    ] {
        assert_eq!(
            e.numeric_expression_with_source(
                &json!({"kind":"lifeTotal","player":reference}),
                "p0",
                None,
            ),
            None
        );
    }
    assert_eq!(
        e.numeric_expression_with_source(&json!({"kind":"lifeTotal"}), "p0", None),
        Some(18)
    );
}
