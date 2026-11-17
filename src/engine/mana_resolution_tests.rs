use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":type_line,"rules":rules})).unwrap()
}
fn engine() -> GameEngine {
    let filler = definition("filler", "Land", vec![]);
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
        640,
    )
    .unwrap();
    e.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("executor", "Enchantment", vec![]),
        0,
    ));
    e.record_event(
        "retainedManaProofPrefix",
        None,
        None,
        json!({"sentinel":"keepCase"}),
    );
    e
}
fn gain(n: i32) -> Value {
    json!({"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":n}})
}
fn add(value: Value) -> Value {
    json!({"kind":"addMana","player":{"kind":"abilityController"},"mana":value})
}
fn program(value: Value) -> Value {
    json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[gain(2),add(value),gain(1)]})
}
fn execution(
    rule: Value,
    decisions: BTreeMap<String, Value>,
) -> super::super::CanonicalRuleExecution {
    assert!(rule_is_executable(&rule));
    super::super::CanonicalRuleExecution {
        controller_id: "player-0".into(),
        source_definition_id: "executor".into(),
        rule,
        decisions,
        targets: BTreeMap::new(),
    }
}
struct ObserveManaChoice {
    refuse: bool,
    calls: usize,
}
impl DecisionProvider for ObserveManaChoice {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        self.calls += 1;
        assert_eq!(
            state.players[0].life, 22,
            "the earlier gain really executed before this callback"
        );
        assert_eq!(request.kind, DecisionKind::ResolutionChoice);
        assert!(request.id.ends_with(":mana"));
        assert_eq!(
            request.source_card.as_ref().unwrap().definition.id,
            "executor"
        );
        if self.refuse {
            Err(EngineError::new("late mana choice refusal"))
        } else {
            Ok(0)
        }
    }
    fn choose_number(
        &mut self,
        _: &GameState,
        _: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        Err(EngineError::new("unexpected number callback"))
    }
}

#[test]
fn mana_preflight_is_shallow_and_shared_visitor_owns_wrappers() {
    let fixed = add(json!("{G}"));
    let dynamic = add(json!({"kind":"chooseColor","amount":1}));
    assert!(!node_requires_checkpoint(&fixed));
    assert!(node_requires_checkpoint(&dynamic));
    let wrapped = json!({"kind":"conditional","then":[dynamic]});
    assert!(!node_requires_checkpoint(&wrapped));
    assert!(super::super::resolution_preflight::requires_checkpoint(
        &wrapped
    ));
}

#[test]
fn public_mana_refusal_observes_earlier_gain_then_restores_it_and_history() {
    let mut e = engine();
    let before = serde_json::to_value(&e.state).unwrap();
    let sequence = e.branching_decision_sequence.get();
    let mut provider = ObserveManaChoice {
        refuse: true,
        calls: 0,
    };
    let error = e
        .execute_canonical_rule(
            execution(
                program(json!({"kind":"chooseColor","amount":1})),
                BTreeMap::new(),
            ),
            &mut provider,
        )
        .unwrap_err();
    assert!(error.message.contains("late mana choice refusal"));
    assert_eq!(provider.calls, 1);
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(e.branching_decision_sequence.get(), sequence);
}

#[test]
fn public_mana_quantities_use_runtime_bindings_in_nested_choices() {
    let value = json!({"kind":"chooseOne","options":[
        {"kind":"fixedMana","symbol":"G","amount":{"kind":"decisionResult","decisionId":"manaAmount"}},"{U}"]});
    for amount in [0, 2] {
        let mut e = engine();
        let mut provider = ObserveManaChoice {
            refuse: false,
            calls: 0,
        };
        e.execute_canonical_rule(
            execution(
                program(value.clone()),
                BTreeMap::from([("manaAmount".into(), json!(amount))]),
            ),
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.calls, 1);
        assert_eq!(
            e.state.players[0].life, 23,
            "later instruction still executes"
        );
        let symbols = e.state.players[0]
            .mana_pool
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>();
        assert_eq!(symbols, vec!["G"; amount as usize]);
        assert_eq!(
            e.state
                .events
                .iter()
                .filter(|v| v.kind == "manaAdded")
                .count(),
            usize::from(amount > 0)
        );
    }
    let mut e = engine();
    let before = serde_json::to_value(&e.state).unwrap();
    let mut provider = ObserveManaChoice {
        refuse: false,
        calls: 0,
    };
    let error = e
        .execute_canonical_rule(execution(program(value), BTreeMap::new()), &mut provider)
        .unwrap_err();
    assert!(error.message.contains("mana.execution.numeric-binding"));
    assert_eq!(provider.calls, 0);
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn public_undefined_mana_is_a_successful_instruction_without_zero_event() {
    let mut e = engine();
    let mut provider = ObserveManaChoice {
        refuse: true,
        calls: 0,
    };
    e.execute_canonical_rule(
        execution(
            program(json!({"kind":"chooseCommanderColor","amount":1})),
            BTreeMap::new(),
        ),
        &mut provider,
    )
    .unwrap();
    assert_eq!(provider.calls, 0);
    assert_eq!(e.state.players[0].life, 23);
    assert!(e.state.players[0].mana_pool.is_empty());
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|v| v.kind == "manaAdded")
            .count(),
        0
    );
    // Direct instruction proof; it makes no activation-illegality claim.
}

#[test]
fn immediate_mana_followup_failure_restores_cost_production_and_prior_effects() {
    let mut e = engine();
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[add(json!("{G}")),gain(1),
            {"kind":"forEachPlayer","players":{"kind":"eachPlayer"},"bind":"p",
             "effects":[add(json!({"kind":"fixedMana","symbol":"G",
                "amount":{"kind":"decisionResult","decisionId":"missingInFollowup"}}))]}]});
    assert!(rule_is_executable(&rule));
    e.state.players[0].battlefield[0].definition = definition("executor", "Artifact", vec![rule]);
    let activation = super::super::CanonicalManaAbilityActivation {
        controller_id: "player-0".into(),
        source_definition_id: "executor".into(),
        mana_symbols: Some(vec!["G".into()]),
    };
    let action = e
        .legal_priority_actions(0)
        .into_iter()
        .find(|a| a.decisions.get("manaAbility").and_then(Value::as_bool) == Some(true))
        .expect("actual legal activation exists before proof");
    assert!(e.immediate_mana_action_requires_checkpoint(&action));
    let before = serde_json::to_value(&e.state).unwrap();
    let mut provider = ObserveManaChoice {
        refuse: true,
        calls: 0,
    };
    let error = e
        .activate_canonical_mana_ability(&activation, &mut provider)
        .unwrap_err();
    assert!(error.message.contains("mana.execution.numeric-binding"));
    assert_eq!(
        serde_json::to_value(&e.state).unwrap(),
        before,
        "restore tap cost, primary G, earlier life gain, and exact event prefix"
    );
    assert_eq!(provider.calls, 0);

    e.state.players[0].battlefield[0].definition = definition(
        "executor",
        "Artifact",
        vec![json!({"kind":"manaAbility","source":{"kind":"self"},
          "costs":[{"kind":"tap","object":{"kind":"self"}}],
          "effects":[add(json!({"kind":"chooseColor","amount":1}))]})],
    );
    let action = e
        .legal_priority_actions(0)
        .into_iter()
        .find(|a| a.decisions.get("manaAbility").and_then(Value::as_bool) == Some(true))
        .expect("pure legacy main production remains discoverable");
    assert!(
        e.immediate_mana_action_requires_checkpoint(&action),
        "structured main production now shares the fallible post-cost boundary"
    );
}
