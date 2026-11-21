use super::*;

fn position() -> (GameEngine, StackObject) {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"source", "name":"Source", "typeLine":"Enchantment", "manaCost":"{0}", "rules":[]
    }))
    .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..3)
                .map(|index| PlayerDeck {
                    id: format!("p{index}"),
                    name: format!("P{index}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 12],
                })
                .collect(),
        },
        11203,
    )
    .unwrap();
    let mut card = card_instance("p0", &definition, 800);
    engine.prepare_battlefield_entry_identity(&mut card);
    engine.state.players[0].battlefield.push(card.clone());
    let stack = StackObject {
        id: "source-reference-test".into(),
        controller: "p0".into(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn effect() -> Value {
    json!({"kind":"dealDamage", "amount":2,
        "recipient":{"kind":"opponentsOf", "player":{"kind":"abilityController"}}})
}

#[test]
fn typed_self_and_ability_source_execute_against_the_retained_source_identity() {
    for kind in ["self", "abilitySource"] {
        let (mut engine, stack) = position();
        let mut rule = effect();
        rule["source"] = json!({"kind":kind});
        assert!(
            engine
                .execute_player_set_damage(&rule, &stack, &BTreeMap::new(), &BTreeMap::new())
                .unwrap()
        );
        assert_eq!(
            engine
                .state
                .players
                .iter()
                .map(|p| p.life)
                .collect::<Vec<_>>(),
            [20, 18, 18]
        );
        assert_eq!(
            engine
                .state
                .events
                .iter()
                .filter(|e| e.kind == "damageDealt")
                .count(),
            2
        );
    }
}

#[test]
fn explicit_missing_or_unknown_sources_are_not_substituted_with_self() {
    for source in [
        json!({"kind":"chosenTarget", "id":"missingSource"}),
        json!({"kind":"unimplementedSource"}),
        Value::Null,
    ] {
        let (mut engine, stack) = position();
        let before = engine.state.clone();
        let mut rule = effect();
        rule["source"] = source;
        let error = engine
            .execute_player_set_damage(&rule, &stack, &BTreeMap::new(), &BTreeMap::new())
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("source reference is unresolved or not an object")
        );
        assert_eq!(engine.state, before);
    }
}

#[test]
fn omitted_source_retains_the_existing_implicit_source_contract() {
    let (mut engine, stack) = position();
    engine
        .execute_player_set_damage(&effect(), &stack, &BTreeMap::new(), &BTreeMap::new())
        .unwrap();
    assert_eq!(engine.state.players[1].life, 18);
    assert_eq!(engine.state.players[2].life, 18);
}
