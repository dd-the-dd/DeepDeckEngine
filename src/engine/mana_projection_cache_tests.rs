use super::*;

fn engine(stateful: bool) -> GameEngine {
    let definition = |id: &str, rules: Vec<Value>| {
        serde_json::from_value::<CardDefinition>(
            json!({"id":id,"name":id,"typeLine":"Land","rules":rules}),
        )
        .unwrap()
    };
    let filler = definition("filler", vec![]);
    let mut e = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        646,
    )
    .unwrap();
    let add = json!({"kind":"addMana","player":{"kind":"abilityController"},"mana":"{G}"});
    let effects = if stateful {
        vec![
            json!({"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}),
            add,
        ]
    } else {
        vec![add]
    };
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],"effects":effects});
    assert!(rule_is_executable(&rule));
    e.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("anchor", vec![rule]),
        0,
    ));
    e
}

#[test]
fn immutable_value_frame_builds_and_reuses_one_lazy_projection() {
    let e = engine(false);
    let frame = ManaProjectionContext::for_value_interpretation(&e);
    let source = &e.state.players[0].battlefield[0];
    frame
        .executable_outputs(&json!("{G}"), "player-0", source)
        .unwrap();
    assert!(
        frame.lazy_projection.get().is_none(),
        "fixed output does not inspect unrelated producers"
    );
    let query = json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1});
    assert_eq!(
        frame
            .executable_outputs(&query, "player-0", source)
            .unwrap(),
        ExecutableManaOutputs::Choices(vec![vec!["G".into()]])
    );
    let first = frame.projected_context().unwrap();
    let second = frame.projected_context().unwrap();
    assert!(
        std::ptr::eq(first, second),
        "repeated queries use the same fixed point"
    );
}

#[test]
fn lazy_projection_failure_is_cached_as_failure_not_empty_type_set() {
    let e = engine(true);
    let frame = ManaProjectionContext::for_value_interpretation(&e);
    let query = json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1});
    let source = &e.state.players[0].battlefield[0];
    let first = frame
        .executable_outputs(&query, "player-0", source)
        .unwrap_err();
    let second = frame
        .executable_outputs(&query, "player-0", source)
        .unwrap_err();
    assert_eq!(first, second);
    assert_eq!(first.state, "mana.projection.program");
    assert!(first.detail.contains("StatefulPrefix"));
    assert!(matches!(frame.lazy_projection.get(), Some(Err(_))));
}
