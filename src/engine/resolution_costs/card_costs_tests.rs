use super::*;

fn position() -> (GameEngine, StackObject) {
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"base", "name":"Base", "typeLine":"Artifact", "manaCost":"{0}", "rules":[]
    }))
    .unwrap();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![definition.clone(); 12],
                })
                .collect(),
        },
        77121,
    )
    .unwrap();
    let source = card_instance("p1", &definition, 500);
    engine.state.players[1].battlefield.push(source.clone());
    let stack = StackObject {
        id: "stack:cost-source".into(),
        controller: "p1".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

fn creature(id: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id":id, "name":id, "typeLine":"Creature", "manaCost":"{0}",
        "power":"3", "toughness":"3",
        "rules":[{"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"shroud"}}]
    }))
    .unwrap()
}

#[test]
fn explicit_payer_and_numeric_context_are_independent_of_the_source_controller() {
    let (engine, source) = position();
    let cost = json!({"kind":"payLife","amount":{"kind":"decisionResult","decisionId":"amount"}});
    let choices = engine
        .prepare_card_cost_options(
            &cost,
            "p0",
            &source,
            &BTreeMap::new(),
            &BTreeMap::from([("amount".into(), json!(2))]),
        )
        .unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].player_id, "p0");
    assert_eq!(source.controller, "p1");
    assert_eq!(choices[0].decisions["costLifePayment"], json!(2));
    assert_eq!(
        choices[0].card_instance_id.as_deref(),
        Some(source.card.instance_id.as_str())
    );
    assert!(choices[0].targets.is_empty() && choices[0].target_order.is_empty());
    assert!(
        engine
            .prepare_card_cost_options(&cost, "p0", &source, &BTreeMap::new(), &BTreeMap::new(),)
            .is_err()
    );
    assert!(
        engine
            .prepare_card_cost_options(
                &json!({"kind":"payLife","amount":21}),
                "p0",
                &source,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
            .unwrap()
            .is_empty()
    );
}

#[test]
fn card_cost_choices_use_payer_resources_without_targeting_restrictions() {
    let (mut engine, source) = position();
    let definition = creature("eligible");
    let payer_card = card_instance("p0", &definition, 501);
    let payer_id = payer_card.instance_id.clone();
    engine.state.players[0].battlefield.push(payer_card);
    engine.state.players[1]
        .battlefield
        .push(card_instance("p1", &definition, 502));
    engine.state.players[0]
        .hand
        .push(card_instance("p0", &definition, 503));
    engine.state.players[1]
        .hand
        .push(card_instance("p1", &definition, 504));
    let filter = json!({"kind":"cardTypeContains","value":"Creature"});
    let sacrifice = json!({"kind":"sacrificePermanent","where":filter});
    let choices = engine
        .prepare_card_cost_options(
            &sacrifice,
            "p0",
            &source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(choices.len(), 1);
    assert_eq!(
        choices[0].decisions["costSacrificePermanent"],
        json!(payer_id)
    );
    assert!(choices[0].targets.is_empty() && choices[0].target_order.is_empty());
    // An explicit null predicate means any permanent; absent selection is invalid.
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &source.card.definition, 506));
    let any_permanent = engine
        .prepare_card_cost_options(
            &json!({"kind":"sacrificePermanent","where":null}),
            "p0",
            &source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(any_permanent.len(), 2);
    assert!(any_permanent.iter().all(|option| option.player_id == "p0"
        && option.targets.is_empty()
        && option.target_order.is_empty()));
    assert!(
        engine
            .prepare_card_cost_options(
                &json!({"kind":"sacrificePermanent"}),
                "p0",
                &source,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
            .is_err()
    );
    let discard = engine
        .prepare_card_cost_options(
            &json!({"kind":"discardCard","where":filter}),
            "p0",
            &source,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
    assert_eq!(discard.len(), 1);
    assert_eq!(
        discard[0].decisions["costDiscardCard"],
        json!(engine.state.players[0].hand[0].instance_id)
    );
    assert!(discard[0].targets.is_empty() && discard[0].target_order.is_empty());
    engine.state.players[0].battlefield[0]
        .flags
        .insert("phasedOut".into(), true);
    assert!(
        engine
            .prepare_card_cost_options(
                &sacrifice,
                "p0",
                &source,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
            .unwrap()
            .is_empty()
    );
}

struct NoChoices {
    calls: usize,
}
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        self.calls += 1;
        Err(EngineError::new(
            "card cost commitment must not make another choice",
        ))
    }
}

#[test]
fn sacrifice_commitment_revalidates_control_before_mutation() {
    let (mut engine, source) = position();
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &creature("eligible"), 505));
    let cost =
        json!({"kind":"sacrificePermanent","where":{"kind":"cardTypeContains","value":"Creature"}});
    let payment = engine
        .prepare_card_cost_options(&cost, "p0", &source, &BTreeMap::new(), &BTreeMap::new())
        .unwrap()
        .remove(0);
    engine.state.players[0].battlefield[0].controller = "p1".into();
    let before = serde_json::to_value(&engine.state).unwrap();
    let mut provider = NoChoices { calls: 0 };
    let error = engine
        .commit_card_cost_payment(&cost, &payment, &source, &mut provider)
        .unwrap_err();
    assert!(error.message.contains("no longer payable"));
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    assert_eq!(provider.calls, 0);
}
