use super::*;
use rand::RngCore;

#[test]
fn fixed_and_symbolic_cost_programs_have_explicit_checked_domains() {
    for cost in ["{0}", "{2}{U}", "{S}{W/U}", "{B/P}", "{2147483647}"] {
        assert!(
            CostProgram::decode(&json!({"kind":"payMana","manaCost":cost})).is_ok(),
            "{cost}"
        );
    }
    for cost in [
        "{X}",
        "{2/W}",
        "{NOPE}",
        "{2147483647}{1}",
        "{2147483647}{R}",
        "{foo{1}",
        "garbage{2}",
        "{-1}",
    ] {
        assert!(
            CostProgram::decode(&json!({"kind":"payMana","manaCost":cost})).is_err(),
            "{cost}"
        );
    }
    assert!(CostProgram::decode(&json!({"kind":"payMana","genericAmount":{"kind":"decisionResult","decisionId":"xValue"}})).is_ok());
    assert!(
        CostProgram::decode(&json!({"kind":"payMana","manaCost":"{2}","genericAmount":2})).is_err()
    );
    assert!(CostProgram::decode(&json!({"kind":"payLife","amount":2})).is_ok());
    assert!(CostProgram::decode(&json!({"kind":"behold","where":{"kind":"cardTypeContains","value":"Creature"},"count":1})).is_err());
    assert!(!effect_supported(
        &json!({"kind":"counterStackObjectUnlessPays","spell":{"kind":"chosenTarget","id":"s"},"manaCost":"{1}"})
    ));
}

#[test]
fn required_cost_result_is_not_an_implicit_decline_and_ids_are_local() {
    let condition = json!({"kind":"not","operand":{"kind":"costWasPaid","decisionId":"payment0","value":true,"required":true}});
    assert!(validate_required_results(&condition, &BTreeMap::new()).is_err());
    assert!(
        validate_required_results(&condition, &BTreeMap::from([("payment0".into(), json!(0))]))
            .is_err()
    );
    assert!(
        validate_required_results(
            &condition,
            &BTreeMap::from([("payment0".into(), json!(false))])
        )
        .is_ok()
    );
    assert!(
        validate_required_results(
            &json!({"kind":"decisionWas","decisionId":"oldFlag"}),
            &BTreeMap::new()
        )
        .is_ok()
    );
    assert!(super::super::resolution_preflight::requires_checkpoint(
        &json!({"effects":[{"kind":"conditional","then":[{"kind":"tryPayCost"}]}]})
    ));
}

struct Refuse {
    calls: usize,
}
impl DecisionProvider for Refuse {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        self.calls += 1;
        Err(EngineError::new("payment-provider-error"))
    }
}

fn position() -> GameEngine {
    let definition: CardDefinition = serde_json::from_value(
        json!({"id":"source","name":"source","typeLine":"Artifact","manaCost":"{0}","rules":[]}),
    )
    .unwrap();
    let mut filler = definition.clone();
    filler.id = "filler".into();
    filler.name = "Filler".into();
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
        },
        99122,
    )
    .unwrap();
    let mut source = card_instance("p0", &definition, 999);
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.state.players[0].battlefield.push(source);
    engine.state.players[0].mana_pool.push(FloatingMana {
        symbol: "C".into(),
        spend_restriction: None,
    });
    engine
}

#[test]
fn shared_public_checkpoint_restores_rng_frames_and_prior_effects_on_payment_error() {
    let mut engine = position();
    let mut before = engine.clone();
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
        {"kind":"shuffleZone","zone":{"kind":"library","player":{"kind":"abilityController"}}},
        {"kind":"tryPayCost","id":"payment0","player":{"kind":"abilityController"},"cost":{"kind":"payMana","manaCost":"{1}"}}
    ]});
    assert!(rule_is_executable(&rule));
    let mut provider = Refuse { calls: 0 };
    let error = engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "p0".into(),
                source_definition_id: "source".into(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut provider,
        )
        .unwrap_err();
    assert!(error.message.contains("payment-provider-error"));
    assert_eq!(provider.calls, 1);
    assert_eq!(
        serde_json::to_value(&engine.state).unwrap(),
        serde_json::to_value(&before.state).unwrap()
    );
    assert_eq!(engine.rng.next_u64(), before.rng.next_u64());
    assert_eq!(
        engine.target_numeric_contexts,
        before.target_numeric_contexts
    );
    assert!(!engine.quantity_transaction_in_progress());
}

#[test]
fn reused_or_captured_result_id_refuses_before_a_payment_callback() {
    let mut engine = position();
    let before = engine.clone();
    let mut provider = Refuse { calls: 0 };
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"tryPayCost","id":"payment0","player":{"kind":"abilityController"},"cost":{"kind":"payMana","manaCost":"{1}"}}]});
    let error = engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "p0".into(),
                source_definition_id: "source".into(),
                rule,
                decisions: BTreeMap::from([("payment0".into(), json!(false))]),
                targets: BTreeMap::new(),
            },
            &mut provider,
        )
        .unwrap_err();
    assert!(
        error
            .message
            .contains("duplicate or captured cost result id")
    );
    assert_eq!(provider.calls, 0);
    assert_eq!(
        serde_json::to_value(&engine.state).unwrap(),
        serde_json::to_value(&before.state).unwrap()
    );
}

#[test]
fn missing_required_result_rolls_back_prior_effects_without_a_provider_call() {
    let mut engine = position();
    let before = engine.clone();
    let mut provider = Refuse { calls: 0 };
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
        {"kind":"gainLife","player":{"kind":"abilityController"},"amount":2},
        {"kind":"conditional","condition":{"kind":"costWasPaid","decisionId":"missingPayment","required":true},"then":[],"else":[]}
    ]});
    let error = engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "p0".into(),
                source_definition_id: "source".into(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut provider,
        )
        .unwrap_err();
    assert!(error.message.contains("required cost result"));
    assert_eq!(provider.calls, 0);
    assert_eq!(
        serde_json::to_value(&engine.state).unwrap(),
        serde_json::to_value(&before.state).unwrap()
    );
    assert_eq!(
        engine.target_numeric_contexts,
        before.target_numeric_contexts
    );
}

#[test]
fn string_and_typed_mana_transports_share_identical_search_results() {
    let engine = position();
    let session = crate::engine::mana_discovery_session::ManaDiscoverySession::new(&engine);
    for count in [0, 1, 2] {
        let fixed = session
            .payment_for_mana_costs(0, &[format!("{{{count}}}")], &BTreeSet::new(), None)
            .unwrap();
        let typed = session
            .payment_for_mana_requirement(
                0,
                ManaRequirement {
                    generic: count,
                    ..Default::default()
                },
                &BTreeSet::new(),
                None,
            )
            .unwrap();
        assert_eq!(
            fixed.as_ref().map(mana_payment_decision),
            typed.as_ref().map(mana_payment_decision)
        );
        assert_eq!(
            fixed.as_ref().map(|plan| &plan.pool_indices),
            typed.as_ref().map(|plan| &plan.pool_indices)
        );
    }
}

struct First;
impl DecisionProvider for First {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Ok(0)
    }
}

#[test]
fn copying_before_an_unsupported_retarget_restores_public_stack_and_private_sequences() {
    let mut engine = position();
    let creature = |id: &str| -> CardDefinition {
        serde_json::from_value(json!({"id":id,"name":id,"typeLine":"Creature","manaCost":"{0}","power":"2","toughness":"2","rules":[]})).unwrap()
    };
    let friendly = creature("friendly");
    let mut warded = creature("warded");
    warded.rules = vec![
        json!({"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"ward","cost":{"kind":"behold","where":{"kind":"cardTypeContains","value":"Creature"},"count":1}}}),
    ];
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &friendly, 501));
    engine.state.players[1]
        .battlefield
        .push(card_instance("p1", &warded, 502));
    let mut original: CardDefinition = serde_json::from_value(
        json!({"id":"original","name":"Original","typeLine":"Instant","manaCost":"{0}","rules":[]}),
    )
    .unwrap();
    original.rules = vec![
        json!({"kind":"spellAbility","source":{"kind":"self"},"declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"recipient","minimum":1,"maximum":1,"candidates":{"kind":"permanents","controller":{"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}},"where":{"kind":"cardTypeContains","value":"Creature"}}}]},"effects":[{"kind":"tapPermanent","permanent":{"kind":"chosenTarget","id":"recipient"}}]}),
    ];
    let mut copier = original.clone();
    copier.id = "copier".into();
    copier.name = "Copier".into();
    copier.rules = vec![
        json!({"kind":"spellAbility","source":{"kind":"self"},"declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"spell","minimum":1,"maximum":1,"candidates":{"kind":"spells"}}]},"effects":[{"kind":"copyStackItem","object":{"kind":"chosenTarget","id":"spell"},"controller":{"kind":"abilityController"},"mayChooseNewTargets":true}]}),
    ];
    engine.state.players[1]
        .hand
        .push(card_instance("p1", &original, 503));
    engine.state.players[0]
        .hand
        .push(card_instance("p0", &copier, 504));
    // These are newly constructed fixture objects, not query-time repairs.
    engine.admit_explicit_test_fixture_objects().unwrap();
    for (actor, id) in [("p1", "original"), ("p0", "copier")] {
        engine
            .cast_canonical_spell(
                &CanonicalSpellCast {
                    controller_id: actor.into(),
                    source_definition_id: id.into(),
                    use_alternative_cost: false,
                    alternative_exile_definition_id: None,
                    declaration_decisions: BTreeMap::new(),
                },
                &mut First,
            )
            .unwrap();
    }
    let mut before = engine.clone();
    let error = engine.resolve_canonical_top_stack(&mut First).unwrap_err();
    assert!(
        error
            .message
            .contains("unsupported resolution cost program")
    );
    assert_eq!(
        serde_json::to_value(&engine.state).unwrap(),
        serde_json::to_value(&before.state).unwrap()
    );
    assert_eq!(engine.copy_sequence, before.copy_sequence);
    assert_eq!(engine.stack_sequence, before.stack_sequence);
    assert_eq!(engine.rng.next_u64(), before.rng.next_u64());
    assert_eq!(
        engine.target_numeric_contexts,
        before.target_numeric_contexts
    );
}

struct Pay;
impl DecisionProvider for Pay {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .position(|option| option.kind == ActionKind::ChooseResolution)
            .unwrap_or(0))
    }
}

#[test]
fn optional_life_cost_waits_for_sba_until_after_real_spell_resolution() {
    let mut engine = position();
    engine.state.players[0].life = 2;
    let definition: CardDefinition = serde_json::from_value(json!({
        "id":"life-payment-program", "name":"Life payment program", "typeLine":"Instant", "manaCost":"{0}",
        "rules":[{"kind":"spellAbility","source":{"kind":"self"},"effects":[{
            "kind":"optionalPayCostPerformEffects","player":{"kind":"abilityController"},
            "cost":{"kind":"payLife","amount":2},
            "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":3}],
            "declineEffects":[]
        }]}]
    })).unwrap();
    assert!(rule_is_executable(&definition.rules[0]));
    engine.state.players[0]
        .hand
        .push(card_instance("p0", &definition, 601));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    engine
        .cast_canonical_spell(
            &CanonicalSpellCast {
                controller_id: "p0".into(),
                source_definition_id: definition.id.clone(),
                use_alternative_cost: false,
                alternative_exile_definition_id: None,
                declaration_decisions: BTreeMap::new(),
            },
            &mut First,
        )
        .unwrap();
    engine.resolve_canonical_top_stack(&mut Pay).unwrap();
    assert_eq!(engine.state.players[0].life, 3);
    assert_eq!(engine.state.players[1].life, 20);
    assert!(!engine.state.players[0].has_lost);
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "lifeLost"
                && event.player_id.as_deref() == Some("p0")
                && event.detail["amount"] == 2)
    );
}

#[test]
fn life_zero_is_payable_even_below_zero_without_emitting_life_loss() {
    let mut engine = position();
    engine.state.players[0].life = -1;
    assert!(engine.player_can_pay_life_cost("p0", 0));
    let events = engine.state.events.len();
    engine.pay_life_as_cost("p0", "source", 0).unwrap();
    assert_eq!(engine.state.players[0].life, -1);
    assert_eq!(engine.state.events.len(), events);
    assert!(!engine.state.players[0].has_lost);
}

#[test]
fn activated_cost_choices_are_excluded_from_ward_target_capability_checks() {
    let mut engine = position();
    let mut source = engine.state.players[0].battlefield[0].clone();
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},
        "costs":[{"kind":"sacrificePermanent","permanent":{"kind":"chosenTarget","id":"costObject"}}],
        "effects":[{"kind":"tapPermanent","permanent":{"kind":"chosenTarget","id":"effectObject"}}]});
    source.definition.rules = vec![rule.clone()];
    engine.state.players[0].battlefield[0] = source.clone();
    let ward_definition: CardDefinition=serde_json::from_value(json!({"id":"unsupported-ward","name":"Unsupported ward","typeLine":"Creature","manaCost":"{0}","power":"2","toughness":"2","rules":[{"kind":"keywordAbility","source":{"kind":"self"},"ability":{"kind":"ward","cost":{"kind":"behold","where":{"kind":"cardTypeContains","value":"Creature"},"count":1}}}]})).unwrap();
    let ward = card_instance("p1", &ward_definition, 602);
    engine.state.players[1].battlefield.push(ward.clone());
    let mut action = LegalAction {
        id: "cost-choice-guard".into(),
        kind: ActionKind::ActivateAbility,
        player_id: "p0".into(),
        label: "guard test".into(),
        card_instance_id: Some(source.instance_id.clone()),
        payment_sources: vec![],
        decisions: BTreeMap::from([("abilityRuleIndex".into(), json!(0))]),
        targets: BTreeMap::from([
            (
                "costObject".into(),
                TargetRef::Permanent {
                    instance_id: ward.instance_id.clone(),
                },
            ),
            (
                "effectObject".into(),
                TargetRef::Permanent {
                    instance_id: source.instance_id.clone(),
                },
            ),
        ]),
        target_order: vec!["costObject".into(), "effectObject".into()],
        attacker_id: None,
        blocker_id: None,
    };
    assert_eq!(
        GameEngine::activated_effect_targets(&rule, &action.targets).len(),
        1
    );
    engine
        .validate_action_target_cost_programs(&action)
        .unwrap();
    action.targets.insert(
        "effectObject".into(),
        TargetRef::Permanent {
            instance_id: ward.instance_id,
        },
    );
    assert!(
        engine
            .validate_action_target_cost_programs(&action)
            .unwrap_err()
            .message
            .contains("unsupported resolution cost program")
    );
}
