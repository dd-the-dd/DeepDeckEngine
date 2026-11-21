use super::*;

fn engine() -> GameEngine {
    let definition = |id: &str| {
        serde_json::from_value::<CardDefinition>(
            json!({"id":id,"name":id,"typeLine":"Land","rules":[]}),
        )
        .unwrap()
    };
    let filler = definition("filler");
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
        648,
    )
    .unwrap();
    for (id, mana) in [
        ("first", "{G}{G}"),
        ("second", "{U}{U}{U}"),
        ("tapped", "{W}{W}{W}{W}{W}"),
    ] {
        let mut d = definition(id);
        d.rules = vec![json!({"kind":"manaAbility","source":{"kind":"self"},
            "costs":[{"kind":"tap","object":{"kind":"self"}}],
            "effects":[{"kind":"addMana","player":{"kind":"abilityController"},"mana":mana}]})];
        assert!(rule_is_executable(&d.rules[0]));
        let mut card = card_instance("player-0", &d, 0);
        card.tapped = id == "tapped";
        e.state.players[0].battlefield.push(card);
    }
    e.state.players[0].mana_pool.push(FloatingMana {
        symbol: "C".into(),
        spend_restriction: None,
    });
    e
}

#[test]
fn shared_budget_preserves_pool_plus_sum_of_source_maxima_and_tap_filter() {
    let e = engine();
    assert_eq!(e.maximum_potential_mana("player-0", None), 6);
    let context = ManaProjectionContext::for_value_interpretation(&e);
    assert_eq!(
        e.checked_mana_budget_in_context("player-0", None, &context)
            .unwrap(),
        6
    );
    let mut visited = Vec::new();
    let maximum = e
        .try_maximum_potential_mana_with("player-0", None, &mut |source, spending| {
            visited.push(source.definition.id.clone());
            Ok::<_, EngineError>(e.mana_choices_for_permanent(source, spending, false, false))
        })
        .unwrap();
    assert_eq!(maximum, 6);
    assert_eq!(visited, ["first", "second"]);
}

#[test]
fn budget_error_is_not_a_partial_or_zero_budget() {
    let e = engine();
    let before = serde_json::to_value(&e.state).unwrap();
    let failure = e
        .try_maximum_potential_mana_with("player-0", None, &mut |source, spending| {
            if source.definition.id == "second" {
                return Err(EngineError::new("unsupported second budget source"));
            }
            Ok(e.mana_choices_for_permanent(source, spending, false, false))
        })
        .unwrap_err();
    assert_eq!(failure.message, "unsupported second budget source");
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
