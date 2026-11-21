use super::*;

fn definition(id: &str, type_line: &str, rules: Vec<Value>) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: type_line.into(),
        mana_cost: String::new(),
        power: Some("2".into()),
        toughness: Some("2".into()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules,
    }
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", vec![]); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        6392,
    )
    .unwrap()
}

fn add(engine: &mut GameEngine, id: &str, type_line: &str, rules: Vec<Value>) -> CardInstance {
    let card = card_instance(
        "player-0",
        &definition(id, type_line, rules),
        engine.state.players[0].battlefield.len() + 100,
    );
    engine.state.players[0].battlefield.push(card.clone());
    engine.admit_explicit_test_fixture_objects().unwrap();
    engine.state.players[0].battlefield.last().unwrap().clone()
}

fn modifier(where_filter: Value, extra: i32) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"multiplyTriggeredAbility","sources":{
            "kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},
            "where":where_filter},"additionalTriggers":extra}]})
}

fn death_rule() -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"permanentDied","object":{"kind":"self"}},
        "effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},
                    "amount":{"kind":"integer","value":1}}]})
}

#[test]
fn effective_source_eligibility_is_captured_before_type_and_multiplier_departure() {
    let mut e = engine();
    add(
        &mut e,
        "multiplier",
        "Artifact",
        vec![modifier(
            json!({"kind":"and","operands":[{"kind":"isLegendary"},
                {"kind":"cardTypeContains","value":"Creature"}]}),
            1,
        )],
    );
    let source = add(&mut e, "source", "Artifact", vec![death_rule()]);
    add(
        &mut e,
        "red-source-multiplier",
        "Artifact",
        vec![modifier(json!({"kind":"colorContains","value":"Red"}), 2)],
    );
    assert_eq!(
        e.additional_trigger_count(&source, &death_rule(), &BTreeMap::new())
            .unwrap(),
        0
    );
    e.state
        .rule_modifiers
        .push(json!({"kind":"temporaryCardType",
        "objectId":source.instance_id,"cardType":"Creature"}));
    assert_eq!(
        e.additional_trigger_count(&source, &death_rule(), &BTreeMap::new())
            .unwrap(),
        0
    );
    e.state
        .rule_modifiers
        .push(json!({"kind":"ringEmblem","playerId":"player-0",
        "bearerId":source.instance_id,"level":1}));
    assert_eq!(
        e.additional_trigger_count(&source, &death_rule(), &BTreeMap::new())
            .unwrap(),
        1
    );

    let (owner, index) = e.permanent_position(&source.instance_id).unwrap();
    e.state.players[owner].battlefield[index]
        .definition
        .rules
        .push(json!({"kind":"rulesMarker","text":"","colorIdentity":["R"]}));
    let source = e.state.players[owner].battlefield[index].clone();
    // A commander's identity is not its actual colors.
    assert_eq!(
        e.additional_trigger_count(&source, &death_rule(), &BTreeMap::new())
            .unwrap(),
        1
    );
    e.state.rule_modifiers.push(
        json!({"kind":"temporaryColors","cardInstanceId":source.instance_id,
        "expiresAfterTurn":e.state.turn_number,"colors":["R"]}),
    );
    assert_eq!(
        e.additional_trigger_count(&source, &death_rule(), &BTreeMap::new())
            .unwrap(),
        3
    );

    let frame = e.capture_death_trigger_source_snapshot();
    let captured = frame
        .sources()
        .iter()
        .find(|c| c.source.instance_id == source.instance_id)
        .unwrap();
    // Printed Artifact has neither characteristic. The captured effective
    // eligibility must survive complete loss of the world that established it.
    e.state.players[0].battlefield.clear();
    e.state.rule_modifiers.clear();
    assert_eq!(
        e.additional_trigger_count_from_snapshot(
            &captured.source,
            &death_rule(),
            &BTreeMap::new(),
            &captured.multiplicity,
            CapturedOccurrence::Death,
        )
        .unwrap(),
        3
    );
}

#[test]
fn event_where_is_applied_to_the_actual_occurrence_not_frozen_as_a_final_count() {
    let mut e = engine();
    let mut entry_only = modifier(Value::Null, 3);
    entry_only["modifiers"][0]["eventWhere"] = json!({"kind":"permanentEntered",
        "where":{"kind":"cardTypeContains","value":"Creature"}});
    add(&mut e, "entry-multiplier", "Artifact", vec![entry_only]);
    add(
        &mut e,
        "ordinary-multiplier",
        "Artifact",
        vec![modifier(Value::Null, 1)],
    );
    let source = add(&mut e, "event-source", "Creature", vec![death_rule()]);
    let frame = e.capture_death_trigger_source_snapshot();
    let captured = frame
        .sources()
        .iter()
        .find(|c| c.source.instance_id == source.instance_id)
        .unwrap();
    let mut rule = death_rule();
    rule["event"] = json!({"kind":"oneOf","events":[
        {"kind":"permanentEntered","player":{"kind":"controllerOf","object":{"kind":"self"}},
         "where":{"kind":"cardTypeContains","value":"Creature"}},
        {"kind":"permanentDied","object":{"kind":"self"}}]});
    let decisions = BTreeMap::from([("triggeringPermanentId".into(), json!(source.instance_id))]);
    assert_eq!(
        e.additional_trigger_count_from_snapshot(
            &source,
            &rule,
            &decisions,
            &captured.multiplicity,
            CapturedOccurrence::Live
        )
        .unwrap(),
        4
    );
    for occurrence in [
        CapturedOccurrence::Death,
        CapturedOccurrence::Sacrifice,
        CapturedOccurrence::Departure,
        CapturedOccurrence::Destruction,
    ] {
        assert_eq!(
            e.additional_trigger_count_from_snapshot(
                &source,
                &rule,
                &decisions,
                &captured.multiplicity,
                occurrence
            )
            .unwrap(),
            1
        );
    }
    // An unsupported color-layer ordering is captured as a Result fact.
    // It matters for the entry occurrence, not for a disjoint death event.
    e.state.players[0].battlefield[0].definition.rules[0]["modifiers"][0]["sources"]["where"] =
        json!({"kind":"colorContains","value":"Red"});
    for color in ["R", "G"] {
        e.state.rule_modifiers.push(
            json!({"kind":"temporaryColors","cardInstanceId":source.instance_id,
            "expiresAfterTurn":e.state.turn_number,"colors":[color]}),
        );
    }
    let frame = e.capture_death_trigger_source_snapshot();
    let captured = frame
        .sources()
        .iter()
        .find(|c| c.source.instance_id == source.instance_id)
        .unwrap();
    assert!(
        e.additional_trigger_count_from_snapshot(
            &source,
            &rule,
            &decisions,
            &captured.multiplicity,
            CapturedOccurrence::Live
        )
        .is_err()
    );
    assert_eq!(
        e.additional_trigger_count_from_snapshot(
            &source,
            &rule,
            &decisions,
            &captured.multiplicity,
            CapturedOccurrence::Death
        )
        .unwrap(),
        1
    );
}

struct NoProviderCalls {
    calls: usize,
}
impl DecisionProvider for NoProviderCalls {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        self.calls += 1;
        Err(EngineError::new(
            "unexpected provider after failed trigger publication",
        ))
    }
}

#[test]
fn count_arithmetic_and_resource_errors_precede_any_stack_allocation() {
    let mut pure = engine();
    pure.draw_cards("player-0", 1).unwrap();
    assert_eq!(pure.quantity_preflight_count(), 1);
    assert_eq!(pure.quantity_snapshot_count(), 0);
    assert!(
        checked_occurrence_count(i32::MAX)
            .unwrap_err()
            .to_string()
            .contains("integer bound")
    );
    assert!(checked_trigger_sum([i32::MAX, 1]).is_err());
    assert_eq!(checked_occurrence_count(65_535).unwrap(), 65_536);
    assert!(
        checked_occurrence_count(65_536)
            .unwrap_err()
            .to_string()
            .contains("resource budget")
    );
    let mut e = engine();
    add(
        &mut e,
        "max-multiplier",
        "Artifact",
        vec![modifier(Value::Null, i32::MAX)],
    );
    let source = add(&mut e, "watcher", "Creature", vec![death_rule()]);
    let before = e.state.clone();
    let sequence = e.stack_sequence;
    let error = e
        .with_quantity_transaction(|e| {
            assert!(matches!(
                e.put_triggered_ability_on_stack(&source, death_rule()),
                TriggerPublicationOutcome::Failed
            ));
            assert!(e.state.stack.is_empty());
            assert_eq!(e.stack_sequence, sequence);
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("integer bound"));
    assert_eq!(e.state, before);
    assert_eq!(e.stack_sequence, sequence);
    assert!(e.check_quantity_failure().is_ok());
    assert_eq!(e.quantity_preflight_count(), 1);
    assert_eq!(e.quantity_snapshot_count(), 1);
}

#[test]
fn combined_modifiers_abort_public_draw_and_clean_retry_keeps_real_draw() {
    let mut e = engine();
    let mut draw_rule = death_rule();
    draw_rule["event"] =
        json!({"kind":"cardDrawn","player":{"kind":"controllerOf","object":{"kind":"self"}}});
    let first = add(
        &mut e,
        "large-first",
        "Artifact",
        vec![modifier(Value::Null, i32::MAX - 1)],
    );
    let second = add(
        &mut e,
        "large-second",
        "Artifact",
        vec![modifier(Value::Null, 2)],
    );
    add(&mut e, "draw-watcher", "Creature", vec![draw_rule]);
    let before = e.state.clone();
    let sequence = e.stack_sequence;
    let error = e.draw_cards("player-0", 2).unwrap_err();
    assert!(error.to_string().contains("integer bound"));
    assert_eq!(e.state, before);
    assert_eq!(e.stack_sequence, sequence);
    e.state.players[0]
        .battlefield
        .retain(|c| c.instance_id != first.instance_id && c.instance_id != second.instance_id);
    e.draw_cards("player-0", 1).unwrap();
    assert_eq!(e.state.players[0].hand.len(), 1);
    assert_eq!(e.state.stack.len(), 1);
}

#[test]
fn public_turn_aborts_step_production_before_provider_and_restores_phase() {
    let mut e = engine();
    add(
        &mut e,
        "step-max",
        "Artifact",
        vec![modifier(Value::Null, i32::MAX)],
    );
    let mut rule = death_rule();
    rule["event"] = json!({"kind":"stepBegan","step":"upkeep","player":{"kind":"controllerOf","object":{"kind":"self"}}});
    add(&mut e, "upkeep-watcher", "Creature", vec![rule]);
    let before = e.state.clone();
    let sequence = e.stack_sequence;
    let mut provider = NoProviderCalls { calls: 0 };
    let error = e.run_next_turn(&mut provider, 1).unwrap_err();
    assert!(error.to_string().contains("integer bound"));
    assert_eq!(provider.calls, 0);
    assert_eq!(e.state, before);
    assert_eq!(e.stack_sequence, sequence);
}

#[test]
fn non_battlefield_source_and_new_incarnation_do_not_acquire_permanent_multiplier() {
    let mut e = engine();
    add(
        &mut e,
        "presence-multiplier",
        "Artifact",
        vec![modifier(Value::Null, 1)],
    );
    let original = add(
        &mut e,
        "presence-source",
        "Legendary Creature",
        vec![death_rule()],
    );
    assert_eq!(
        e.additional_trigger_count(&original, &death_rule(), &BTreeMap::new())
            .unwrap(),
        1
    );
    let card = e.state.players[0].battlefield.pop().unwrap();
    e.state.players[0].command_zone.push(card);
    assert_eq!(
        e.additional_trigger_count(&original, &death_rule(), &BTreeMap::new())
            .unwrap(),
        0
    );
    let mut returned = e.state.players[0].command_zone.pop().unwrap();
    e.prepare_battlefield_entry_identity(&mut returned);
    e.state.players[0].battlefield.push(returned.clone());
    assert_eq!(
        e.additional_trigger_count(&original, &death_rule(), &BTreeMap::new())
            .unwrap(),
        0
    );
    assert_eq!(
        e.additional_trigger_count(&returned, &death_rule(), &BTreeMap::new())
            .unwrap(),
        1
    );
    e.state.players[0]
        .battlefield
        .last_mut()
        .unwrap()
        .flags
        .retain(|k, _| !k.starts_with(zone_change_triggers::TRACKED_ZONE_PREFIX));
    assert!(
        e.additional_trigger_count(&returned, &death_rule(), &BTreeMap::new())
            .is_err()
    );
}

#[test]
fn step_producer_keeps_granted_object_and_delayed_abilities_distinct() {
    let mut e = engine();
    add(
        &mut e,
        "step-origin-multiplier",
        "Artifact",
        vec![modifier(Value::Null, 1)],
    );
    let mut object_rule = death_rule();
    object_rule["event"] = json!({"kind":"stepBegan","step":"upkeep","player":{"kind":"controllerOf","object":{"kind":"self"}}});
    let source = add(&mut e, "step-origin-source", "Creature", vec![]);
    // Granted ability is attached by the existing typed grant boundary.
    let mut grant = modifier(Value::Null, 1);
    grant["modifiers"][0] = json!({"kind":"grantTriggeredAbility","objects":{"kind":"permanents",
        "controller":{"kind":"controllerOf","object":{"kind":"self"}},
        "where":{"kind":"cardTypeContains","value":"Creature"}},"ability":object_rule});
    add(&mut e, "grantor", "Artifact", vec![grant]);
    e.state
        .rule_modifiers
        .push(json!({"kind":"delayedStepTrigger","step":"upkeep",
        "sourceCard":source,"controllerId":"player-0","effects":[{"kind":"gainLife",
            "player":{"kind":"abilityController"},"amount":3}]}));
    e.with_quantity_transaction(|e| {
        e.enqueue_step_triggers("upkeep");
        e.check_quantity_failure()
    })
    .unwrap();
    assert_eq!(e.state.stack.len(), 3);
    assert_eq!(
        e.state
            .stack
            .iter()
            .filter(|s| s.ability_rule.as_ref().unwrap()["effects"][0]["amount"] == json!(3))
            .count(),
        1
    );
    assert_eq!(
        e.state
            .stack
            .iter()
            .filter(|s| s.ability_rule.as_ref().unwrap()["effects"][0]["amount"]
                == json!({"kind":"integer","value":1}))
            .count(),
        2
    );
}

#[test]
fn ring_causal_bearer_does_not_own_the_emblem_attack_ability() {
    let mut e = engine();
    add(
        &mut e,
        "ring-origin-multiplier",
        "Artifact",
        vec![modifier(
            json!({"kind":"cardTypeContains","value":"Creature"}),
            1,
        )],
    );
    let mut object_rule = death_rule();
    object_rule["event"] = json!({"kind":"declaredAttacker","object":{"kind":"self"}});
    let mut source = add(&mut e, "ring-origin-source", "Creature", vec![object_rule]);
    source.flags.insert("attacking".into(), true);
    *e.state.players[0].battlefield.last_mut().unwrap() = source.clone();
    let mut watcher_rule = death_rule();
    watcher_rule["event"] = json!({"kind":"attackingCreatureAbilityTriggered",
        "player":{"kind":"controllerOf","object":{"kind":"self"}}});
    assert!(triggered_rule_supported(&watcher_rule));
    watcher_rule["effects"][0]["amount"] = json!({"kind":"integer","value":7});
    add(
        &mut e,
        "attacking-ability-watcher",
        "Artifact",
        vec![watcher_rule],
    );
    e.state.rule_modifiers.push(
        json!({"kind":"ringEmblem","playerId":"player-0","bearerId":source.instance_id,"level":2}),
    );
    e.with_quantity_transaction(|e| {
        e.enqueue_declared_attacker_triggers(&[source.instance_id.clone()]);
        e.check_quantity_failure()
    })
    .unwrap();
    assert_eq!(e.state.stack.len(), 5);
    assert_eq!(
        e.state
            .stack
            .iter()
            .filter(|s| s.ability_rule.as_ref().unwrap()["effects"][0]["kind"] == "drawCards")
            .count(),
        1
    );
    assert_eq!(
        e.state
            .stack
            .iter()
            .filter(|s| s.ability_rule.as_ref().unwrap()["effects"][0]["amount"]
                == json!({"kind":"integer","value":1}))
            .count(),
        2
    );
    let watched = e
        .state
        .stack
        .iter()
        .filter(|s| {
            s.ability_rule.as_ref().unwrap()["effects"][0]["amount"]
                == json!({"kind":"integer","value":7})
        })
        .map(|s| {
            s.decisions["triggeringStackId"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(watched.len(), 2);
    for id in watched {
        assert!(e.state.stack.iter().any(|s| s.id == id
            && s.card.instance_id == source.instance_id
            && s.ability_rule.as_ref().unwrap()["effects"][0]["amount"]
                == json!({"kind":"integer","value":1})));
    }
}

#[test]
fn resource_error_without_any_multiplier_rolls_back_public_draw_and_retries_cleanly() {
    let mut e = engine();
    let mut rule = death_rule();
    rule["event"] =
        json!({"kind":"cardDrawn","player":{"kind":"controllerOf","object":{"kind":"self"}}});
    add(&mut e, "plain-draw-watcher", "Artifact", vec![rule]);
    e.stack_sequence = usize::MAX;
    let before = e.state.clone();
    let snapshots = e.quantity_snapshot_count();
    let error = e.draw_cards("player-0", 1).unwrap_err();
    assert!(error.to_string().contains("identity sequence exhausted"));
    assert_eq!(e.state, before);
    assert_eq!(e.stack_sequence, usize::MAX);
    assert_eq!(e.quantity_snapshot_count(), snapshots + 1);
    assert!(e.check_quantity_failure().is_ok());
    e.stack_sequence = 0;
    e.draw_cards("player-0", 1).unwrap();
    assert_eq!(e.state.players[0].hand.len(), 1);
    assert_eq!(e.state.stack.len(), 1);

    // The accepted legacy top-level keyword form also constructs a real
    // triggered ability; it must not evade the same source preflight.
    let mut e = engine();
    add(
        &mut e,
        "raw-prowess",
        "Creature",
        vec![json!({"kind":"prowess"})],
    );
    let mut spell = definition(
        "zero-instant",
        "Instant",
        vec![json!({"kind":"spellAbility",
        "source":{"kind":"self"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]})],
    );
    spell.mana_cost = "{0}".into();
    e.state.players[0]
        .hand
        .push(card_instance("player-0", &spell, 900));
    e.admit_explicit_test_fixture_objects().unwrap();
    e.state.step = GameStep::PrecombatMain;
    e.stack_sequence = usize::MAX - 1;
    let before = e.state.clone();
    let request = rule_execution::CanonicalSpellCast {
        controller_id: "player-0".into(),
        source_definition_id: "zero-instant".into(),
        use_alternative_cost: false,
        alternative_exile_definition_id: None,
        declaration_decisions: BTreeMap::new(),
    };
    let mut provider = NoProviderCalls { calls: 0 };
    let error = e.cast_canonical_spell(&request, &mut provider).unwrap_err();
    assert!(error.to_string().contains("identity sequence exhausted"));
    assert_eq!(e.state, before);
    assert_eq!(e.stack_sequence, usize::MAX - 1);
    assert_eq!(provider.calls, 0);
    assert!(e.check_quantity_failure().is_ok());
    e.stack_sequence = 0;
    e.cast_canonical_spell(&request, &mut provider).unwrap();
    assert_eq!(e.state.stack.len(), 2);
}

#[test]
fn unproved_origin_requires_support_only_when_a_multiplier_actually_applies() {
    let mut e = engine();
    let multiplier = add(
        &mut e,
        "unproved-disjoint",
        "Artifact",
        vec![modifier(
            json!({"kind":"cardTypeContains","value":"Land"}),
            1,
        )],
    );
    let source = add(&mut e, "unproved-source", "Creature", vec![]);
    e.with_quantity_transaction(|e| {
        let result = e.put_triggered_ability_on_stack_with_origin(
            &source,
            death_rule(),
            BTreeMap::new(),
            TriggerOrigin::Unproven,
        );
        assert!(matches!(result, TriggerPublicationOutcome::Published(_)));
        Ok(())
    })
    .unwrap();
    assert_eq!(e.state.stack.len(), 1);
    let (p, i) = e.permanent_position(&multiplier.instance_id).unwrap();
    e.state.players[p].battlefield[i].definition.rules = vec![modifier(Value::Null, 1)];
    let before = e.state.clone();
    let error = e
        .with_quantity_transaction(|e| {
            let result = e.put_triggered_ability_on_stack_with_origin(
                &source,
                death_rule(),
                BTreeMap::new(),
                TriggerOrigin::Unproven,
            );
            assert!(matches!(result, TriggerPublicationOutcome::Failed));
            Ok(())
        })
        .unwrap_err();
    assert!(error.to_string().contains("UnsupportedTriggerProvenance"));
    assert_eq!(e.state, before);
}
