use super::*;

fn definition(id: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":"Land","rules":rules})).unwrap()
}
fn engine() -> GameEngine {
    let filler = definition("filler", vec![]);
    GameEngine::new(
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
        645,
    )
    .unwrap()
}
fn source(id: &str, value: Value) -> CardInstance {
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"abilityController"},"mana":value}]});
    assert!(rule_is_executable(&rule));
    card_instance("player-0", &definition(id, vec![rule]), 0)
}

#[test]
fn checked_counterfactual_query_feeds_exact_executable_payment_quantities() {
    let mut e = engine();
    e.state.players[0]
        .battlefield
        .push(source("anchor", json!("{G}")));
    e.state.players[0].battlefield.push(source(
        "query",
        json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":2}),
    ));
    let plan = e
        .try_checked_mana_activation_plan(&e.state.players[0].battlefield[1], None, true, true)
        .unwrap();
    let declarations = plan.into_activation_choices();
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].symbols, ["G", "G"]);
    let payment = e
        .try_checked_payment_for_mana_costs_with_context(
            0,
            &["{G}{G}".into()],
            &BTreeSet::from([e.state.players[0].battlefield[0].instance_id.clone()]),
            None,
            None,
            None,
        )
        .unwrap()
        .expect("query production pays two green");
    assert_eq!(payment.activations.len(), 1);
    assert_eq!(payment.activations[0].symbols, ["G", "G"]);
    assert_eq!(
        payment.activations[0].source_id,
        e.state.players[0].battlefield[1].instance_id
    );
}

#[test]
fn undefined_recursive_query_keeps_activation_but_is_not_a_payment_source() {
    let mut e = engine();
    e.state.players[0].battlefield.push(source(
        "unanchored",
        json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1}),
    ));
    let plan = e
        .try_checked_mana_activation_plan(&e.state.players[0].battlefield[0], None, true, true)
        .unwrap();
    assert!(matches!(
        &plan.choices[..],
        [CheckedManaActivationChoice::NoMana(_)]
    ));
    let declarations = plan.into_activation_choices();
    assert_eq!(declarations.len(), 1);
    assert!(
        declarations[0].symbols.is_empty(),
        "CR106.7 undefined type does not erase ability under605"
    );
    let payment = e
        .try_checked_payment_for_mana_costs_with_context(
            0,
            &["{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
        )
        .expect("semantic NoMana is not unsupported");
    assert!(payment.is_none());
    // This tests the internal declaration contract, not the held public API.
}

#[test]
fn missing_amount_is_an_error_through_checked_planner_and_real_payment_kernel() {
    let mut e = engine();
    e.state.players[0].battlefield.push(source(
        "unbound",
        json!({"kind":"fixedMana","symbol":"G",
        "amount":{"kind":"decisionResult","decisionId":"absent"}}),
    ));
    let before = serde_json::to_value(&e.state).unwrap();
    let failure = e
        .try_checked_payment_for_mana_costs_with_context(
            0,
            &["{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(failure.message.contains("mana.execution.numeric-binding"));
    assert!(
        failure
            .message
            .contains(&e.state.players[0].battlefield[0].instance_id)
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn mana_for_other_recipients_is_not_accepted_as_controller_payment() {
    let mut e = engine();
    let mut gift = source("gift", json!("{G}"));
    gift.definition.rules[0]["effects"][0]["player"] = json!({"kind":"eachPlayer"});
    assert!(rule_is_executable(&gift.definition.rules[0]));
    e.state.players[0].battlefield.push(gift);
    let failure = e
        .try_checked_payment_for_mana_costs_with_context(
            0,
            &["{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(failure.message.contains("mana.discovery.recipient"));
    e.state.players[0].battlefield[0]
        .flags
        .insert("phasedOut".into(), true);
    assert!(
        e.try_checked_payment_for_mana_costs_with_context(
            0,
            &["{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None
        )
        .expect("phased-out object is not a candidate")
        .is_none()
    );
}
