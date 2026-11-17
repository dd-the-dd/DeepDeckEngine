use super::*;

#[test]
fn generated_payment_validates_selected_cost_instead_of_imposing_tap() {
    for requires_tap in [false, true] {
        let costs = if requires_tap {
            json!([{"kind":"tap","object":{"kind":"self"}}])
        } else {
            json!([{"kind":"sacrificePermanent","permanent":{"kind":"self"}}])
        };
        let def: CardDefinition=serde_json::from_value(json!({"id":"source","name":"Generic producer","typeLine":"Artifact","rules":[{
            "kind":"manaAbility","source":{"kind":"self"},"costs":costs,
            "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},"mana":"{G}"}]
        }]})).unwrap();
        let mut e = GameEngine::new(
            GameSetup {
                opening_hand_size: 0,
                starting_player: 0,
                players: (0..2)
                    .map(|i| PlayerDeck {
                        id: format!("player-{i}"),
                        name: format!("P{i}"),
                        starting_life: 20,
                        cards: vec![def.clone(); 2],
                    })
                    .collect(),
            },
            91,
        )
        .unwrap();
        let mut source = card_instance("player-0", &def, 0);
        source.tapped = true;
        let source_id = source.instance_id.clone();
        e.state.players[0].battlefield.push(source);
        let action:LegalAction=serde_json::from_value(json!({"id":"plan","kind":"castSpell","playerId":"player-0","label":"validation only",
            "decisions":{"manaPayment":[{"sourceId":source_id,"ruleIndex":0,"mana":["G"],"lifePaid":0,"sacrificesSource":!requires_tap}]}})).unwrap();
        let before = e.state.clone();
        let result = e.validate_generated_mana_payment(&action);
        if requires_tap {
            assert!(result.unwrap_err().message.contains("cannot pay tap cost"));
        } else {
            result.expect("a selected sacrifice-only cost has no tap requirement");
        }
        assert_eq!(e.state, before, "validation never pays or mutates any cost");
    }
}
