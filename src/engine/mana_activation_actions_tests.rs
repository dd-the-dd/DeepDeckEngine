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
        647,
    )
    .unwrap()
}
fn source(id: &str, mana: Value) -> CardInstance {
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"abilityController"},"mana":mana}]});
    assert!(rule_is_executable(&rule));
    card_instance("player-0", &definition(id, vec![rule]), 0)
}
fn no_declaration(
    rule: &Value,
    _: &str,
    _: &str,
    _: &ManaProjectionContext<'_>,
) -> Result<Option<Vec<CastingDeclaration>>, EngineError> {
    assert!(
        rule.get("declaration").is_none(),
        "test callback covers only no-declaration rules"
    );
    Ok(Some(vec![CastingDeclaration {
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
        target_order: Vec::new(),
    }]))
}

#[test]
fn extracted_legacy_generator_preserves_order_identifiers_and_metadata() {
    let mut e = engine();
    e.state.players[0].battlefield.extend([
        source("first", json!("{G}")),
        source("second", json!("{U}")),
    ]);
    let actions = e.legacy_mana_activation_actions(0);
    assert_eq!(actions.len(), 2);
    for (i, symbol) in ["G", "U"].iter().enumerate() {
        let card = &e.state.players[0].battlefield[i];
        let action = &actions[i];
        assert_eq!(action.id, format!("mana:{}:0:0:0", card.instance_id));
        assert_eq!(
            action.label,
            format!("Tap {}: Add {{{symbol}}}", card.definition.name)
        );
        assert_eq!(
            action.card_instance_id.as_deref(),
            Some(card.instance_id.as_str())
        );
        assert_eq!(action.decisions["manaProduced"], json!([symbol]));
        assert_eq!(action.decisions["manaAbility"], json!(true));
        assert_eq!(action.decisions["tapSource"], json!(true));
        assert_eq!(action.decisions["abilityRuleIndex"], json!(0));
        assert_eq!(
            action.decisions["manaPayment"],
            mana_payment_decision(&ManaPayment::default())
        );
        assert!(action.payment_sources.is_empty());
        assert!(action.targets.is_empty());
        assert!(action.target_order.is_empty());
    }
}

#[test]
fn checked_generator_publishes_a_real_zero_output_activation_declaration() {
    let mut e = engine();
    e.state.players[0].battlefield.push(source(
        "unanchored",
        json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":1}),
    ));
    let actions = e
        .try_checked_mana_activation_actions(0, &mut no_declaration)
        .unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].kind, ActionKind::ActivateAbility);
    assert_eq!(actions[0].decisions["manaProduced"], json!([]));
    assert_eq!(actions[0].decisions["tapSource"], json!(true));
    assert_eq!(actions[0].decisions["manaAbility"], json!(true));
    // Publication in the private generator is not a public activation/runtime proof.
}

#[test]
fn checked_production_failure_is_not_published_as_no_actions() {
    let mut e = engine();
    e.state.players[0].battlefield.push(source(
        "unbound",
        json!({"kind":"fixedMana","symbol":"G",
        "amount":{"kind":"decisionResult","decisionId":"absent"}}),
    ));
    let error = e
        .try_checked_mana_activation_actions(0, &mut no_declaration)
        .unwrap_err();
    assert!(error.message.contains("mana.execution.numeric-binding"));
}

#[test]
fn checked_declaration_failure_reaches_generator_boundary_without_mutation() {
    let mut e = engine();
    e.state.players[0]
        .battlefield
        .push(source("ordinary", json!("{G}")));
    let before = serde_json::to_value(&e.state).unwrap();
    let mut calls = 0;
    let error = e
        .try_checked_mana_activation_actions(0, &mut |_, _, _, _| {
            calls += 1;
            Err(EngineError::new("bounded declaration budget failure"))
        })
        .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(error.message, "bounded declaration budget failure");
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn checked_payment_failure_reaches_generator_boundary_instead_of_omitting_action() {
    let mut e = engine();
    let mut paid = source("paid", json!("{G}"));
    paid.definition.rules[0]["costs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"payMana","manaCost":"{1}"}));
    e.state.players[0].battlefield.extend([
        paid,
        source(
            "unbound",
            json!({"kind":"fixedMana","symbol":"G",
        "amount":{"kind":"decisionResult","decisionId":"absent"}}),
        ),
    ]);
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .try_checked_mana_activation_actions(0, &mut no_declaration)
        .unwrap_err();
    assert!(error.message.contains("mana.execution.numeric-binding"));
    assert!(error.message.contains("unbound"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn checked_generator_rejects_invalid_player_before_indexing_or_callbacks() {
    let e = engine();
    let before = serde_json::to_value(&e.state).unwrap();
    let mut calls = 0;
    let error = e
        .try_checked_mana_activation_actions(usize::MAX, &mut |_, _, _, _| {
            calls += 1;
            Err(EngineError::new("must not reach declaration callback"))
        })
        .unwrap_err();
    assert!(error.message.contains("mana.discovery.player-index"));
    assert!(error.message.contains(&usize::MAX.to_string()));
    assert_eq!(calls, 0);
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}
