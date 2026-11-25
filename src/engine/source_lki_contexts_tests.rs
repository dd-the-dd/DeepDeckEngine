use crate::engine::*;

fn definition(id: &str) -> CardDefinition {
    serde_json::from_value(json!({
        "id":id,"name":id,"typeLine":"Creature","manaCost":"{G}",
        "power":"2","toughness":"2","rules":[]
    }))
    .unwrap()
}
fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player {n}"),
                    starting_life: 20,
                    cards: vec![definition("filler"); 8],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        91,
    )
    .unwrap()
}
fn install(engine: &mut GameEngine, id: &str) -> CardInstance {
    let mut source = card_instance("player-0", &definition(id), 0);
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.state.players[0].battlefield.push(source.clone());
    source
}
fn action(source: &CardInstance) -> LegalAction {
    serde_json::from_value(json!({
        "id":"immediate-source","kind":"activateAbility","playerId":"player-0",
        "label":"immediate source","cardInstanceId":source.instance_id,
    }))
    .unwrap()
}

#[test]
fn immediate_source_observation_does_not_create_gameplay_stack_or_identity() {
    let mut e = engine();
    let source = install(&mut e, "source");
    let before = serde_json::to_value(&e.state).unwrap();
    let identity_counter = e.state.battlefield_incarnation_sequence;
    e.with_immediate_source_context(&action(&source), &source, |e, scope| {
        let context = e.immediate_source_context(scope)?;
        assert_eq!(context.card, source);
        assert_eq!(context.controller, "player-0");
        assert!(e.state.stack.is_empty());
        Ok(())
    })
    .unwrap();
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(e.state.battlefield_incarnation_sequence, identity_counter);
    assert!(e.source_lki_contexts.frames.is_empty());
}

#[test]
fn nested_immediate_source_scopes_restore_outer_observer_after_error() {
    let mut e = engine();
    let source = install(&mut e, "source");
    e.with_immediate_source_context(&action(&source), &source, |e, outer| {
        let nested: Result<(), EngineError> =
            e.with_immediate_source_context(&action(&source), &source, |_, _| {
                Err(EngineError::new("deliberate nested error"))
            });
        assert!(nested.is_err());
        assert_eq!(e.source_lki_contexts.frames.len(), outer + 1);
        assert_eq!(
            e.immediate_source_context(outer)?.card.instance_id,
            source.instance_id
        );
        Ok(())
    })
    .unwrap();
    assert!(e.source_lki_contexts.frames.is_empty());
}

#[test]
fn immediate_source_without_incarnation_is_rejected_without_minting_one() {
    let mut e = engine();
    let source = card_instance("player-0", &definition("source"), 0);
    e.state.players[0].battlefield.push(source.clone());
    let before = serde_json::to_value(&e.state).unwrap();
    let result = e.with_immediate_source_context(&action(&source), &source, |_, _| Ok(()));
    assert!(result.is_err());
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert!(e.source_lki_contexts.frames.is_empty());
}

#[test]
fn departure_of_new_same_id_incarnation_cannot_authenticate_old_frame() {
    let mut e = engine();
    let source = install(&mut e, "source");
    e.with_immediate_source_context(&action(&source), &source, |e, scope| {
        // Simulate an unsupported exit that omitted capture, then a real new
        // incarnation with the same physical id. Its departure cannot repair
        // the missing history of the original object.
        let mut returned = e.state.players[0].battlefield.remove(0);
        e.prepare_battlefield_entry_identity(&mut returned);
        e.state.players[0].battlefield.push(returned);
        e.capture_pending_source_lki(&source.instance_id);
        assert!(!source_lki::source_has_departed(
            &e.source_lki_contexts.frames[scope]
        ));
        assert!(e.immediate_source_context(scope).is_err());
        Ok(())
    })
    .unwrap();
}

#[test]
fn real_departure_retains_old_source_after_same_id_returns() {
    let mut e = engine();
    let source = install(&mut e, "source");
    e.with_immediate_source_context(&action(&source), &source, |e, scope| {
        e.move_permanent_to_graveyard(&source.instance_id, "sacrificed")?;
        let mut returned = e.state.players[0].graveyard.pop().unwrap();
        returned.definition.power = Some("9".into());
        e.prepare_battlefield_entry_identity(&mut returned);
        e.state.players[0].battlefield.push(returned);
        let context = e.immediate_source_context(scope)?;
        assert!(source_lki::source_has_departed(&context));
        assert_eq!(
            e.runtime_numeric_expression(
                &json!({"kind":"powerOf","object":{"kind":"self"}}),
                &context,
                &BTreeMap::new(),
                &BTreeMap::new(),
            ),
            Some(2)
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn postcost_primary_failure_restores_public_activation_costs_and_can_retry() {
    struct NoChoice;
    impl DecisionProvider for NoChoice {
        fn choose(
            &mut self,
            _: &GameState,
            _: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            Err(EngineError::new("provider must not be called"))
        }
    }
    let mut e = engine();
    let rule = json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}},
            {"kind":"removeCounters","permanent":{"kind":"self"},"counter":"charge","count":1}],
        "effects":[{"kind":"addMana","player":{"kind":"abilityController"},
            "mana":{"kind":"fixedMana","symbol":"G","amount":{"kind":"divide","left":1,
                "right":{"kind":"countCounters","object":{"kind":"self"},"counter":"charge"},"round":"down"}}}]});
    assert!(rule_is_executable(&rule));
    let mut source = install(&mut e, "counter-source");
    source.definition.type_line = "Artifact".into();
    source.definition.rules = vec![rule];
    source.counters.insert("charge".into(), 1);
    e.state.players[0].battlefield[0] = source.clone();
    let mut declared = action(&source);
    declared.decisions.extend([
        ("manaAbility".into(), json!(true)),
        ("abilityRuleIndex".into(), json!(0)),
        ("manaProduced".into(), json!(["G"])),
        ("lifePaid".into(), json!(0)),
        ("tapSource".into(), json!(true)),
        ("manaPayment".into(), json!([])),
    ]);
    assert!(e.immediate_mana_action_requires_checkpoint(&declared));
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .apply_priority_action(&declared, &mut NoChoice)
        .unwrap_err();
    assert!(error.message.contains("mana.execution.numeric-binding"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert!(e.source_lki_contexts.frames.is_empty());
    // Change only the declared program's denominator to a valid post-cost
    // value. The same public action can now pay the still-unspent cost once.
    e.state.players[0].battlefield[0].definition.rules[0]["effects"][0]["mana"]["amount"] =
        json!(1);
    e.apply_priority_action(&declared, &mut NoChoice).unwrap();
    assert_eq!(
        e.state.players[0].battlefield[0].counters.get("charge"),
        Some(&0)
    );
    assert!(e.state.players[0].battlefield[0].tapped);
    assert_eq!(
        e.state.players[0]
            .mana_pool
            .iter()
            .map(|mana| mana.symbol.as_str())
            .collect::<Vec<_>>(),
        ["G"]
    );
}

#[test]
fn real_world_planner_backtracks_to_query_before_sacrifice_and_receipt_replays_that_order() {
    let mut e = engine();
    let mut red = install(&mut e, "red-source");
    red.definition.type_line = "Artifact".into();
    red.definition.mana_cost = "{R}".into();
    red.definition.rules = vec![json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}},
            {"kind":"sacrificePermanent","permanent":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"abilityController"},"mana":"{R}"}]})];
    let mut query = install(&mut e, "query-source");
    query.definition.type_line = "Artifact".into();
    query.definition.mana_cost = "{G}".into();
    query.definition.rules = vec![json!({"kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"tap","object":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"abilityController"},
            "mana":{"kind":"eachColorAmongPermanents","player":{"kind":"abilityController"}}}]})];
    for rule in red.definition.rules.iter().chain(&query.definition.rules) {
        assert!(rule_is_executable(rule));
    }
    // The losing sacrifice-first branch is intentionally enumerated first.
    e.state.players[0].battlefield = vec![red.clone(), query.clone()];
    let before = serde_json::to_value(&e.state).unwrap();
    let plan = e
        .try_checked_payment_for_mana_costs_with_context(
            0,
            &["{R}{R}{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
        )
        .unwrap()
        .expect("query first preserves red for the later sacrifice");
    assert!(plan.ordered);
    assert_eq!(
        plan.activations
            .iter()
            .map(|a| a.source_id.as_str())
            .collect::<Vec<_>>(),
        [query.instance_id.as_str(), red.instance_id.as_str()]
    );
    assert_eq!(plan.activations[0].symbols, ["R", "G"]);
    assert_eq!(plan.activations[1].symbols, ["R"]);
    assert_eq!(
        serde_json::to_value(&e.state).unwrap(),
        before,
        "projection is read only"
    );
    let mut payment_action = action(&query);
    payment_action
        .decisions
        .insert("manaPayment".into(), mana_payment_decision(&plan));
    payment_action
        .decisions
        .insert("manaPoolPayment".into(), json!(plan.pool_indices));
    e.apply_payment(0, &payment_action).unwrap();
    assert!(e.state.players[0].mana_pool.is_empty());
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|event| event.kind == "manaAbilityActivated")
            .map(|event| event.card_instance_id.as_deref())
            .collect::<Vec<_>>(),
        [
            Some(query.instance_id.as_str()),
            Some(red.instance_id.as_str())
        ]
    );
}

struct FirstOption;

impl DecisionProvider for FirstOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        (!request.options.is_empty())
            .then_some(0)
            .ok_or_else(|| EngineError::new("departed copy test decision has no option"))
    }
}

/// `Whenever another legendary creature you control dies, create a token
/// that's a copy of that creature` (Ratadrabik of Urborg): the copy reads the
/// event's object as it last existed on the battlefield (CR 608.2h, 707.2),
/// from its authenticated departure capture, never from the card it became.
/// Without a capture of the incarnation the event saw, nothing is copied.
#[test]
fn a_token_copy_of_a_departed_event_object_reads_its_last_known_copiable_values() {
    let mut e = engine();
    let dying = install(&mut e, "dying");
    let ability = install(&mut e, "ability");
    let identity = GameEngine::tracked_zone_identity(&dying)
        .expect("an admitted battlefield incarnation")
        .to_string();
    e.move_permanent_to_graveyard(&dying.instance_id, "destroyed")
        .expect("the creature dies");
    // The card it became is another object; changing it must not change the
    // copied values.
    e.state.players[0]
        .graveyard
        .iter_mut()
        .filter(|card| card.instance_id == dying.instance_id)
        .for_each(|card| card.definition.power = Some("9".into()));
    let effects = json!([{
        "kind": "createTokens",
        "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
        "quantity": { "kind": "integer", "value": 1 },
        "token": {
            "kind": "copyOf",
            "object": { "kind": "triggeringPermanent" },
            "except": [],
        },
    }]);
    let record = |departed_identity: &str| {
        let mut stack = StackObject {
            id: "stack:ability".to_string(),
            controller: "player-0".to_string(),
            card: ability.clone(),
            cant_be_countered: false,
            exile_on_leave_stack: false,
            ability_kind: Some("triggeredAbility".to_string()),
            ability_rule: None,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        };
        stack.decisions.insert(
            "triggeringPermanentId".to_string(),
            json!(dying.instance_id),
        );
        // A death record: the object is no longer on the battlefield when
        // the ability is put on the stack (`capture_event_object`).
        stack.decisions.insert(
            "triggeringPermanentIdentity".to_string(),
            serde_json::Value::Null,
        );
        stack.decisions.insert(
            "triggeringPermanentDepartedIdentity".to_string(),
            json!(departed_identity),
        );
        stack
    };
    let copies = |e: &GameEngine| {
        e.state.players[0]
            .battlefield
            .iter()
            .filter(|card| card.definition.is_token && card.definition.name == "dying")
            .cloned()
            .collect::<Vec<_>>()
    };

    // No capture of this incarnation: nothing is guessed.
    e.execute_effects(
        &effects,
        &record("not-an-incarnation"),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut FirstOption,
    )
    .expect("an uncaptured object copies nothing");
    assert!(copies(&e).is_empty());

    e.execute_effects(
        &effects,
        &record(&identity),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut FirstOption,
    )
    .expect("the copy of the departed object resolves");
    let created = copies(&e);
    assert_eq!(created.len(), 1, "one token copy of the departed creature");
    assert_eq!(created[0].definition.power.as_deref(), Some("2"));
    assert_eq!(created[0].controller, "player-0");
}
