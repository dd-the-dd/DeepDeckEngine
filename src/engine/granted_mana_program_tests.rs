use super::*;

fn program() -> Value {
    json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},
            "mana":{"kind":"chooseColor","amount":1}}]})
}

fn grant() -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"grantActivatedAbility","objects":{"kind":"permanents",
            "controller":{"kind":"controllerOf","object":{"kind":"self"}},
            "where":{"kind":"or","operands":[{"kind":"cardTypeContains","value":"Land"},{"kind":"subtypeContains","value":"Ally"}]}},
            "ability":program()}]})
}

fn engine(holder_type: &str) -> GameEngine {
    let filler: CardDefinition = serde_json::from_value(
        json!({"id":"filler","name":"Filler","typeLine":"Artifact","rules":[]}),
    )
    .unwrap();
    let mut e = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 8],
                })
                .collect(),
        },
        495211,
    )
    .unwrap();
    for (id, typ, rules) in [
        ("grantor", "Creature", vec![grant()]),
        ("holder", holder_type, vec![]),
    ] {
        let definition: CardDefinition = serde_json::from_value(json!({"id":id,"name":id,"typeLine":typ,"manaCost":"{0}","rules":rules,"power":"2","toughness":"2"})).unwrap();
        let mut card = card_instance("p0", &definition, if id == "grantor" { 91 } else { 92 });
        e.prepare_battlefield_entry_identity(&mut card);
        e.state.players[0].battlefield.push(card);
    }
    e
}

fn action() -> LegalAction {
    serde_json::from_value(json!({"id":"declaration","kind":"activateAbility","playerId":"p0","label":"test","cardInstanceId":"holder"})).unwrap()
}

#[test]
fn program_origin_decoding_is_strict_and_native_metadata_stays_unchanged() {
    for bad in [Value::Null, json!(-1), json!("0"), json!(1.5), json!(true)] {
        let mut a = action();
        a.decisions.insert("abilityRuleIndex".into(), bad);
        assert!(declared_origin(&a).is_err());
    }
    let mut a = action();
    insert_origin(
        Some(&ManaProgramOrigin::Native { rule_index: 0 }),
        &mut a.decisions,
    );
    assert_eq!(
        a.decisions,
        BTreeMap::from([("abilityRuleIndex".into(), json!(0))])
    );
    assert_eq!(
        declared_origin(&a).unwrap(),
        Some(ManaProgramOrigin::Native { rule_index: 0 })
    );
    assert!(receipt_origin(&json!({"ruleIndex":"0"})).is_err());
    assert!(
        receipt_origin(
            &json!({"ruleIndex":0,"grantedManaProgram":{"kind":"native","rule_index":0}})
        )
        .is_err()
    );
}

#[test]
fn declared_grant_cannot_forge_program_or_reuse_a_returned_grantor() {
    let mut e = engine("Land");
    let holder = e.state.players[0].battlefield[1].clone();
    let origin = e
        .admitted_mana_programs(&holder)
        .unwrap()
        .pop()
        .unwrap()
        .origin;
    assert_eq!(
        e.resolve_mana_program(&holder, Some(&origin)).unwrap(),
        Some(program())
    );
    let mut forged = origin.clone();
    if let ManaProgramOrigin::Granted { program, .. } = &mut forged {
        program["effects"][0]["mana"]["amount"] = json!(2);
    }
    assert!(e.resolve_mana_program(&holder, Some(&forged)).is_err());
    let mut encoded = json!(origin);
    encoded["unproved"] = json!(true);
    let mut a = action();
    a.decisions.insert("grantedManaProgram".into(), encoded);
    assert!(declared_origin(&a).is_err());
    let mut returned = e.state.players[0].battlefield[0].clone();
    GameEngine::clear_tracked_zone_identity(&mut returned);
    e.prepare_battlefield_entry_identity(&mut returned);
    e.state.players[0].battlefield[0] = returned;
    assert!(e.resolve_mana_program(&holder, Some(&origin)).is_err());
    assert_eq!(e.admitted_mana_programs(&holder).unwrap().len(), 1);
}

#[test]
fn effective_membership_reuses_temporary_and_stable_continuous_subtypes() {
    let mut e = engine("Artifact");
    let holder = e.state.players[0].battlefield[1].clone();
    assert!(e.admitted_mana_programs(&holder).unwrap().is_empty());
    e.state.rule_modifiers.push(json!({"kind":"temporarySubtype","cardInstanceId":holder.instance_id,"subtype":"Ally","expiresAfterTurn":e.state.turn_number}));
    assert_eq!(e.admitted_mana_programs(&holder).unwrap().len(), 1);
    e.state.rule_modifiers.clear();
    assert!(e.admitted_mana_programs(&holder).unwrap().is_empty());
    e.state.players[0].battlefield[1].definition.rules.push(
        json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"addSubtype","subtype":"Ally","objects":{"kind":"self"}}]}),
    );
    let holder = e.state.players[0].battlefield[1].clone();
    assert_eq!(e.admitted_mana_programs(&holder).unwrap().len(), 1);
    e.state.players[0].battlefield[1].definition.rules[0]["modifiers"][0]["objects"] =
        json!({"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}});
    e.state.rule_modifiers.push(
        json!({"kind":"temporaryCardType","objectId":holder.instance_id,"cardType":"Creature"}),
    );
    assert!(
        e.admitted_mana_programs(&e.state.players[0].battlefield[1])
            .unwrap_err()
            .message
            .contains("selector dependency")
    );
}

#[test]
fn selector_ownership_and_unproved_fields_are_never_ignored() {
    let mut e = engine("Land");
    e.state.players[0].battlefield[1].owner = "p1".into();
    let holder = e.state.players[0].battlefield[1].clone();
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["objects"]["ownership"] =
        json!("owned");
    assert!(e.admitted_mana_programs(&holder).unwrap().is_empty());
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["objects"]["ownership"] =
        json!("notOwned");
    assert_eq!(e.admitted_mana_programs(&holder).unwrap().len(), 1);
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["objects"]["condition"] =
        json!(true);
    assert!(e.admitted_mana_programs(&holder).is_err());
}

#[test]
fn ordered_receipt_structure_does_not_guess_future_availability() {
    let e = engine("Land");
    let holder = e.state.players[0].battlefield[1].clone();
    let origin = e
        .admitted_mana_programs(&holder)
        .unwrap()
        .pop()
        .unwrap()
        .origin;
    let mut a = action();
    a.card_instance_id = Some(holder.instance_id.clone());
    a.decisions.insert("manaAbility".into(), json!(true));
    a.decisions.insert("manaProduced".into(), json!(["U"]));
    insert_origin(Some(&origin), &mut a.decisions);
    let receipt = json!({"sourceId":holder.instance_id,"ruleIndex":null,"grantedManaProgram":origin,
        "mana":["U"],"lifePaid":0,"sacrificesSource":false,"activationDeclaration":a});
    assert!(ordered_receipt_declaration(&receipt, "p0").is_ok());
    let mut unavailable = e.clone();
    unavailable.state.players[0].battlefield.remove(0);
    // Structural validation is independent of the availability world; the
    // normal commit resolver must reject it if no earlier prefix restored it.
    let declared = ordered_receipt_declaration(&receipt, "p0").unwrap();
    assert!(
        unavailable
            .resolve_mana_program(&holder, declared_origin(&declared).unwrap().as_ref())
            .is_err()
    );
}

#[test]
fn earlier_cost_removing_grantor_rejects_later_origin_and_rolls_back() {
    struct NoChoice;
    impl DecisionProvider for NoChoice {
        fn choose(
            &mut self,
            _: &GameState,
            _: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            Err(EngineError::new("unexpected provider choice"))
        }
    }
    let mut e = engine("Land");
    e.state.players[0].battlefield[0].definition.type_line = "Creature - Ally".into();
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["ability"]["costs"] = json!([
        {"kind":"sacrificePermanent","permanent":{"kind":"self"}}]);
    let mut land = e.state.players[0].battlefield[1].definition.clone();
    land.id = "second-land".into();
    land.name = "Second Land".into();
    let mut second = card_instance("p0", &land, 93);
    e.prepare_battlefield_entry_identity(&mut second);
    e.state.players[0].battlefield.push(second);
    let spell:CardDefinition=serde_json::from_value(json!({"id":"order-spell","name":"Order Spell","typeLine":"Instant","manaCost":"{U}{U}",
        "rules":[{"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]}]})).unwrap();
    let spell = card_instance("p0", &spell, 94);
    let spell_id = spell.instance_id.clone();
    e.state.players[0].hand.push(spell);
    let source_ids = [
        e.state.players[0].battlefield[0].instance_id.clone(),
        e.state.players[0].battlefield[1].instance_id.clone(),
    ];
    let mana = e.complete_checked_mana_activation_actions(0).unwrap();
    let mut payment = ManaPayment {
        ordered: true,
        ..ManaPayment::default()
    };
    for id in &source_ids {
        let declared = mana
            .iter()
            .find(|a| {
                a.card_instance_id.as_deref() == Some(id.as_str())
                    && a.decisions["manaProduced"] == json!(["U"])
            })
            .unwrap()
            .clone();
        payment.activations.push(ManaActivation {
            origin: declared_origin(&declared).unwrap(),
            declaration: Some(Box::new(declared)),
            source_id: id.clone(),
            symbols: vec!["U".into()],
            life_cost: 0,
            activation_requirement: ManaRequirement::default(),
            sacrifices_source: true,
        });
    }
    let mut cast = e
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .find(|a| {
            a.kind == ActionKind::CastSpell
                && a.card_instance_id.as_deref() == Some(spell_id.as_str())
        })
        .unwrap();
    cast.decisions
        .insert("manaPayment".into(), mana_payment_decision(&payment));
    cast.payment_sources = source_ids.to_vec();
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .apply_checked_priority_action(&cast, &mut NoChoice)
        .unwrap_err();
    assert!(
        error.message.contains("origin") || error.message.contains("no longer legal"),
        "{}",
        error.message
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn unsupported_end_step_payment_restores_schedule_and_public_state() {
    struct NoChoice;
    impl DecisionProvider for NoChoice {
        fn choose(
            &mut self,
            _: &GameState,
            _: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            Err(EngineError::new("unexpected end-step choice"))
        }
    }
    let mut e = engine("Land");
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["objects"]["condition"] =
        json!(true);
    let holder = e.state.players[0].battlefield[1].instance_id.clone();
    e.state.rule_modifiers.push(json!({"kind":"ashlingPayOrSacrificeAtEndStep","cardInstanceId":holder,"playerId":"p0","manaCost":"{U}","notBeforeTurn":e.state.turn_number}));
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e.process_end_step_sacrifices(&mut NoChoice).unwrap_err();
    assert!(
        error.message.contains("selector/domain context"),
        "{}",
        error.message
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn checked_declaration_failure_after_optional_payment_restores_whole_program() {
    struct PayOnce {
        calls: usize,
    }
    impl DecisionProvider for PayOnce {
        fn choose(
            &mut self,
            _: &GameState,
            request: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            self.calls += 1;
            assert_eq!(request.options.len(), 2);
            Ok(1)
        }
    }
    let mut e = engine("Land");
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["objects"]["condition"] =
        json!(true);
    e.state.players[0].counters.insert("energy".into(), 1);
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
        {"kind":"gainLife","player":{"kind":"abilityController"},"amount":1},
        {"kind":"optionalPayCostCreateReflexiveTrigger","player":{"kind":"abilityController"},
         "cost":{"kind":"payPlayerCounters","player":{"kind":"abilityController"},"counter":"energy","count":1},
         "ability":{"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"reflexiveTriggerCreated","object":{"kind":"self"}},
           "declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseNumber","id":"xValue","minimum":0}]},
           "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]}}
    ]});
    assert!(rule_is_executable(&rule));
    assert!(super::resolution_preflight::requires_checkpoint(&rule));
    let request = serde_json::from_value(
        json!({"controllerId":"p0","sourceDefinitionId":"grantor","rule":rule}),
    )
    .unwrap();
    let before = serde_json::to_value(&e.state).unwrap();
    let mut provider = PayOnce { calls: 0 };
    let error = e
        .execute_canonical_rule(request, &mut provider)
        .unwrap_err();
    assert_eq!(
        provider.calls, 1,
        "failure must follow the selected cost payment"
    );
    assert!(
        error.message.contains("selector/domain context"),
        "{}",
        error.message
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
}

#[test]
fn unprojected_type_world_is_an_explicit_cap_before_empty_graph() {
    let mut e = engine("Land");
    e.state.players[0].battlefield[1].definition.type_line = "Artifact".into();
    let holder = e.state.players[0].battlefield[1].instance_id.clone();
    e.state.rule_modifiers.push(json!({"kind":"counterLinkedCharacteristics","cardInstanceId":holder,"counter":"charge","addTypes":["Land"]}));
    e.state.players[0].battlefield[1]
        .counters
        .insert("charge".into(), 1);
    let error = match super::mana_projection_context::ManaProjectionContext::new(&e) {
        Ok(_) => panic!("unproved new Land must not silently disappear from the empty graph"),
        Err(error) => error.into_engine_error(),
    };
    assert!(
        error
            .message
            .contains("counter-linked card-type transformation"),
        "{}",
        error.message
    );
}

#[test]
fn acquired_limit_identity_separates_sites_grantors_and_survives_control_change() {
    struct NoChoice;
    impl DecisionProvider for NoChoice {
        fn choose(
            &mut self,
            _: &GameState,
            _: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            Err(EngineError::new("unexpected choice"))
        }
    }
    let mut e = engine("Land");
    let mut rule = grant();
    rule["modifiers"][0]["objects"]
        .as_object_mut()
        .unwrap()
        .remove("controller");
    let program = &mut rule["modifiers"][0]["ability"];
    program["costs"] = json!([{"kind":"payLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":1}]);
    program["effects"][0]["mana"] = json!("{U}");
    program["activationLimit"] = json!({"kind":"oncePerTurn","id":"limit"});
    let duplicate = rule["modifiers"][0].clone();
    rule["modifiers"].as_array_mut().unwrap().push(duplicate);
    e.state.players[0].battlefield[0].definition.rules = vec![rule.clone()];
    let mut other = e.state.players[0].battlefield[0].definition.clone();
    other.id = "other-grantor".into();
    other.name = "Other Grantor".into();
    other.rules[0]["modifiers"].as_array_mut().unwrap().pop();
    let mut other = card_instance("p0", &other, 95);
    e.prepare_battlefield_entry_identity(&mut other);
    e.state.players[0].battlefield.push(other);
    let holder = e.state.players[0].battlefield[1].instance_id.clone();
    let programs = e
        .admitted_mana_programs(&e.state.players[0].battlefield[1])
        .unwrap();
    assert_eq!(
        programs.len(),
        3,
        "two real sites plus another grantor are distinct abilities"
    );
    for remaining in (1..=3).rev() {
        let choices = e
            .complete_checked_mana_activation_actions(0)
            .unwrap()
            .into_iter()
            .filter(|a| a.card_instance_id.as_deref() == Some(holder.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(choices.len(), remaining);
        e.apply_mana_ability_action(0, &choices[0], &mut NoChoice)
            .unwrap();
    }
    assert_eq!(e.state.players[0].life, 17);
    assert_eq!(e.state.players[0].mana_pool.len(), 3);
    let mut moved = e.state.players[0].battlefield.remove(1);
    moved.controller = "p1".into();
    e.state.players[1].battlefield.push(moved);
    assert!(
        e.complete_checked_mana_activation_actions(1)
            .unwrap()
            .is_empty(),
        "CR602.5b control change does not reset use"
    );
    let mut returned = e.state.players[0].battlefield[0].clone();
    GameEngine::clear_tracked_zone_identity(&mut returned);
    e.prepare_battlefield_entry_identity(&mut returned);
    e.state.players[0].battlefield[0] = returned;
    assert_eq!(
        e.complete_checked_mana_activation_actions(1).unwrap().len(),
        2,
        "new grantor incarnation produces two fresh acquired abilities"
    );
    let native = ManaProgramOrigin::Native { rule_index: 7 };
    assert_eq!(
        mana_activation_limit_flag(
            &json!({"kind":"oncePerTurn","id":"limit"}),
            9,
            Some(&native)
        ),
        Some("limit:turn:9".into())
    );
}

#[test]
fn outer_rule_condition_cap_precedes_printed_boolean_filtering() {
    let mut e = engine("Artifact");
    let holder = e.state.players[0].battlefield[1].clone();
    e.state.rule_modifiers.push(json!({"kind":"temporarySubtype","cardInstanceId":holder.instance_id,"subtype":"Ally","expiresAfterTurn":e.state.turn_number}));
    e.state.players[0].battlefield[0].definition.rules[0]["condition"] =
        json!({"kind":"controlsPermanent","where":{"kind":"subtypeContains","value":"Ally"}});
    let error = e.admitted_mana_programs(&holder).unwrap_err();
    assert!(
        error.message.contains("outer static-rule condition"),
        "{}",
        error.message
    );
    e.state.players[0].battlefield[0].definition.rules[0]["condition"] =
        json!({"kind":"compare","left":0,"operator":">","right":1});
    assert!(
        e.admitted_mana_programs(&holder).unwrap().is_empty(),
        "proved false is genuine absence"
    );
    e.state.players[0].battlefield[0].definition.rules[0]
        .as_object_mut()
        .unwrap()
        .remove("condition");
    let mut transform = json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"condition":{"kind":"controlsPermanent","where":{"kind":"subtypeContains","value":"Ally"}},"modifiers":[{"kind":"addSubtype","objects":{"kind":"self"},"subtype":"Ally"}]});
    e.state.players[0].battlefield[1]
        .definition
        .rules
        .push(transform.take());
    assert!(
        e.admitted_mana_programs(&holder)
            .unwrap_err()
            .message
            .contains("outer static-rule condition")
    );
}

#[test]
fn ordinary_granted_targeted_mana_producer_is_not_counterfactual_absence() {
    let mut e = engine("Land");
    let program = json!({"kind":"activatedAbility","source":{"kind":"self"},
      "costs":[{"kind":"tap","object":{"kind":"self"}}],
      "declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"recipient","minimum":1,"maximum":1,"candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}}}]},
      "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},"mana":"{R}"},{"kind":"untapPermanent","permanent":{"kind":"chosenTarget","id":"recipient"}}]});
    assert!(
        activated_rule_supported(&program),
        "the counterexample is a supported ordinary targeted ability"
    );
    assert!(
        !mana_rule_supported(&program),
        "it cannot enter direct mana-ability discovery"
    );
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["ability"] =
        program.clone();
    let holder = e.state.players[0].battlefield[1].clone();
    assert!(e.admitted_mana_programs(&holder).unwrap().is_empty());
    assert!(
        e.granted_activated_abilities(&holder)
            .iter()
            .any(|a| a["effects"] == program["effects"])
    );
    let error = match super::mana_projection_context::ManaProjectionContext::new(&e) {
        Ok(_) => panic!("CR106.7 ordinary producer must not become a false empty projection"),
        Err(error) => error.into_engine_error(),
    };
    assert!(
        error
            .message
            .contains("counterfactual ordinary activated producer"),
        "{}",
        error.message
    );
    e.state.players[0].battlefield[0].definition.rules[0]["condition"] =
        json!({"kind":"compare","left":0,"operator":">","right":1});
    assert!(
        super::mana_projection_context::ManaProjectionContext::new(&e).is_ok(),
        "proved false outer condition is genuine absence"
    );
    e.state.players[0].battlefield[0].definition.rules[0]["condition"] =
        json!({"kind":"controlsPermanent","where":{"kind":"subtypeContains","value":"Ally"}});
    let error = match super::mana_projection_context::ManaProjectionContext::new(&e) {
        Ok(_) => panic!(
            "unproved outer condition must not hide the ordinary producer before capability validation"
        ),
        Err(error) => error.into_engine_error(),
    };
    assert!(
        error
            .message
            .contains("outer static-rule condition context"),
        "{}",
        error.message
    );
}

#[test]
fn ordered_payment_promotes_passive_preflight_and_restores_private_observers() {
    struct NoChoice;
    impl DecisionProvider for NoChoice {
        fn choose(
            &mut self,
            _: &GameState,
            _: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            Err(EngineError::new("unexpected provider choice"))
        }
    }
    let mut e = engine("Land");
    e.state.players[0].battlefield[0].definition.type_line = "Creature - Ally".into();
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["ability"]["costs"] = json!([
        {"kind":"sacrificePermanent","permanent":{"kind":"self"}}]);
    let spell: CardDefinition = serde_json::from_value(json!({"id":"passive-spell","name":"Passive Spell","typeLine":"Instant","manaCost":"{U}","rules":[{"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]}]})).unwrap();
    let spell = card_instance("p0", &spell, 96);
    let spell_id = spell.instance_id.clone();
    e.state.players[0].hand.push(spell);
    let grantor = e.state.players[0].battlefield[0].clone();
    let holder = e.state.players[0].battlefield[1].clone();
    let mana = e.complete_checked_mana_activation_actions(0).unwrap();
    let grantor_action = mana
        .iter()
        .find(|a| {
            a.card_instance_id.as_deref() == Some(grantor.instance_id.as_str())
                && a.decisions["manaProduced"] == json!(["U"])
        })
        .unwrap()
        .clone();
    let holder_action = mana
        .iter()
        .find(|a| {
            a.card_instance_id.as_deref() == Some(holder.instance_id.as_str())
                && a.decisions["manaProduced"] == json!(["U"])
        })
        .unwrap()
        .clone();
    let mut payment = ManaPayment {
        ordered: true,
        ..ManaPayment::default()
    };
    for declared in [grantor_action.clone(), holder_action] {
        payment.activations.push(ManaActivation {
            source_id: declared.card_instance_id.clone().unwrap(),
            origin: declared_origin(&declared).unwrap(),
            symbols: vec!["U".into()],
            life_cost: 0,
            activation_requirement: ManaRequirement::default(),
            sacrifices_source: true,
            declaration: Some(Box::new(declared)),
        });
    }
    let mut cast = e
        .try_legal_priority_actions(0)
        .unwrap()
        .into_iter()
        .find(|a| {
            a.kind == ActionKind::CastSpell
                && a.card_instance_id.as_deref() == Some(spell_id.as_str())
        })
        .unwrap();
    cast.decisions
        .insert("manaPayment".into(), mana_payment_decision(&payment));
    cast.payment_sources = vec![grantor.instance_id.clone(), holder.instance_id.clone()];
    let observer = super::target_numeric_lki::TargetNumericContext {
        stack_id: "outer-payment-observer".into(),
        targets: BTreeMap::from([(
            "grantor".into(),
            super::target_numeric_lki::NumericTargetBinding {
                identity: super::object_identity::object_identity_of(&grantor).unwrap(),
                last_known: None,
            },
        )]),
        legal_at_start: BTreeSet::from(["grantor".into()]),
        known_targets_at_start: BTreeSet::from(["grantor".into()]),
        ..Default::default()
    };
    e.with_target_numeric_context(observer, |e| {
        e.with_immediate_source_context(&grantor_action, &grantor, |e, index| {
            e.with_quantity_preflight(false, |e| {
                assert!(e.quantity_preflight_in_progress());
                assert!(
                    !e.quantity_transaction_in_progress(),
                    "start inside a passive, not already checkpointed scope"
                );
                let before = e.clone();
                let source_before =
                    serde_json::to_value(e.immediate_source_context(index).unwrap()).unwrap();
                let error = e
                    .apply_checked_priority_action(&cast, &mut NoChoice)
                    .unwrap_err();
                assert!(
                    error.message.contains("origin") || error.message.contains("no longer legal"),
                    "{}",
                    error.message
                );
                assert_eq!(
                    serde_json::to_value(&e.state).unwrap(),
                    serde_json::to_value(&before.state).unwrap()
                );
                assert_eq!(
                    e.target_numeric_contexts, before.target_numeric_contexts,
                    "no departed target-LKI leaks"
                );
                assert_eq!(
                    serde_json::to_value(e.immediate_source_context(index).unwrap()).unwrap(),
                    source_before,
                    "borrowed source observer restored before scope cleanup"
                );
                assert_eq!(e.pending_empty_draws, before.pending_empty_draws);
                assert_eq!(e.commander_last_zones, before.commander_last_zones);
                assert_eq!(e.stack_sequence, before.stack_sequence);
                assert_eq!(e.copy_sequence, before.copy_sequence);
                assert_eq!(
                    e.branching_decision_sequence.get(),
                    before.branching_decision_sequence.get()
                );
                assert_eq!(e.force_end_turn, before.force_end_turn);
                let mut actual_rng = e.rng.clone();
                let mut prior_rng = before.rng.clone();
                assert_eq!(
                    rand::RngCore::next_u64(&mut actual_rng),
                    rand::RngCore::next_u64(&mut prior_rng)
                );
                assert!(e.quantity_preflight_in_progress());
                assert!(!e.quantity_transaction_in_progress());
                assert_eq!(
                    e.quantity_snapshot_count(),
                    before.quantity_snapshot_count() + 1,
                    "one promoted snapshot, no nested clones"
                );
                assert!(
                    e.check_quantity_failure().is_ok(),
                    "failed inner receipt must not poison a restored passive scope"
                );
                Ok(())
            })
        })
    })
    .unwrap();
    assert!(!e.quantity_preflight_in_progress());
    assert!(!e.quantity_transaction_in_progress());
}
