use super::super::target_numeric_lki::{TargetNumericProperty, TargetNumericReadError};
use super::super::*;

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "target numeric scenario unexpectedly requested a choice",
        ))
    }
}
fn definition(id: &str, kind: &str, mana: &str) -> CardDefinition {
    CardDefinition {
        id: id.into(),
        name: id.into(),
        type_line: kind.into(),
        mana_cost: mana.into(),
        power: Some("2".into()),
        toughness: Some("3".into()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: vec![],
    }
}
fn position() -> (GameEngine, StackObject, String) {
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Instant", "{0}"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        16301,
    )
    .unwrap();
    let mut target = card_instance(
        "player-1",
        &definition("target", "Creature - Scout", "{5}"),
        1,
    );
    engine.prepare_battlefield_entry_identity(&mut target);
    let id = target.instance_id.clone();
    engine.state.players[1].battlefield.push(target);
    let reference = TargetRef::Permanent {
        instance_id: id.clone(),
    };
    let mut decisions = BTreeMap::new();
    engine
        .capture_target_incarnation(&mut decisions, "targetPermanent", &reference)
        .expect("test target must have an admitted incarnation");
    let stack = StackObject {
        id: "stack:target-numeric".into(),
        controller: "player-0".into(),
        card: card_instance(
            "player-0",
            &definition("numeric-source", "Sorcery", "{1}{B}"),
            0,
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions,
        targets: BTreeMap::from([("targetPermanent".into(), reference)]),
    };
    (engine, stack, id)
}
fn object() -> Value {
    json!({"kind":"chosenTarget","id":"targetPermanent"})
}
fn destroy() -> Value {
    json!({"kind":"destroyPermanent","permanent":object()})
}
fn numeric(kind: &str) -> Value {
    json!({"kind":kind,"object":object()})
}
fn life(kind: &str, value: &str) -> Value {
    json!({"kind":kind,"player":{"kind":"abilityController"},"amount":numeric(value)})
}
fn buff() -> Value {
    json!({"kind":"modifyPowerToughness","object":object(),"power":{"kind":"integer","value":2},
    "toughness":{"kind":"integer","value":2},"duration":{"kind":"untilEndOfCurrentTurn"}})
}
fn run(
    engine: &mut GameEngine,
    stack: &StackObject,
    effects: Vec<Value>,
) -> Result<(), EngineError> {
    let context = engine.target_numeric_context(stack);
    engine.with_target_numeric_context(context, |engine| {
        engine.execute_effects(
            &Value::Array(effects),
            stack,
            &mut BTreeMap::new(),
            &mut stack.decisions.clone(),
            &mut NoChoices,
        )
    })
}

#[test]
fn destruction_mana_value_is_last_known_and_caster_loses_five() {
    let (mut e, s, id) = position();
    run(&mut e, &s, vec![destroy(), life("loseLife", "manaValueOf")]).unwrap();
    assert_eq!(e.state.players[0].life, 15);
    assert!(e.permanent_position(&id).is_none());
    assert_eq!(e.state.players[1].graveyard.len(), 1);
}

#[test]
fn live_read_follows_modifier_instead_of_binding_at_declaration() {
    let (mut e, s, _) = position();
    run(&mut e, &s, vec![buff(), life("gainLife", "powerOf")]).unwrap();
    assert_eq!(e.state.players[0].life, 24);
}

#[test]
fn failed_indestructible_destroy_reads_surviving_live_value() {
    let (mut e, s, id) = position();
    e.state.players[1].battlefield[0]
        .definition
        .rules
        .push(json!({"kind":"keywordAbility","ability":{"kind":"indestructible"}}));
    run(
        &mut e,
        &s,
        vec![destroy(), buff(), life("gainLife", "powerOf")],
    )
    .unwrap();
    assert!(e.permanent_position(&id).is_some());
    assert_eq!(e.state.players[0].life, 24);
}

#[test]
fn shield_removed_survivor_uses_value_after_followup_modifier() {
    let (mut e, s, id) = position();
    e.state.players[1].battlefield[0]
        .counters
        .insert("shield".into(), 1);
    run(
        &mut e,
        &s,
        vec![destroy(), buff(), life("gainLife", "powerOf")],
    )
    .unwrap();
    let (p, i) = e.permanent_position(&id).unwrap();
    assert_eq!(
        e.state.players[p].battlefield[i]
            .counters
            .get("shield")
            .copied()
            .unwrap_or(0),
        0
    );
    assert_eq!(e.state.players[0].life, 24);
}

#[test]
fn regeneration_survivor_stays_live_and_tapped() {
    let (mut e, s, id) = position();
    e.state.players[1].battlefield[0].damage_marked = 1;
    e.state
        .rule_modifiers
        .push(json!({"kind":"regenerationShield","cardInstanceId":id}));
    run(
        &mut e,
        &s,
        vec![destroy(), buff(), life("gainLife", "powerOf")],
    )
    .unwrap();
    let (p, i) = e.permanent_position(&id).unwrap();
    let card = &e.state.players[p].battlefield[i];
    assert!(card.tapped);
    assert_eq!(card.damage_marked, 0);
    assert_eq!(e.state.players[0].life, 24);
}

#[test]
fn actual_copy_effect_snapshot_uses_effective_mana_not_printed_graveyard_mana() {
    let (mut e, mut s, id) = position();
    e.state.players[1].battlefield[0].definition.mana_cost = "{2}".into();
    e.state.players[1].battlefield[0]
        .printed_definition
        .as_mut()
        .unwrap()
        .mana_cost = "{2}".into();
    let mut model = card_instance(
        "player-1",
        &definition("copy-model", "Creature - Scout", "{5}"),
        2,
    );
    e.prepare_battlefield_entry_identity(&mut model);
    let reference = TargetRef::Permanent {
        instance_id: model.instance_id.clone(),
    };
    e.state.players[1].battlefield.push(model);
    e.capture_target_incarnation(&mut s.decisions, "model", &reference)
        .expect("test target must have an admitted incarnation");
    s.targets.insert("model".into(), reference);
    run(&mut e,&s,vec![json!({"kind":"becomeCopyOfPermanent","object":object(),"copy":{"kind":"chosenTarget","id":"model"}}),destroy(),life("loseLife","manaValueOf")]).unwrap();
    assert_eq!(e.state.players[0].life, 15);
    let departed = e.state.players[1]
        .graveyard
        .iter()
        .find(|c| c.instance_id == id)
        .unwrap();
    assert_eq!(mana_value(&departed.definition), 2);
}

#[test]
fn departed_effective_power_and_toughness_include_modifier() {
    let (mut e, s, _) = position();
    run(
        &mut e,
        &s,
        vec![
            buff(),
            destroy(),
            life("gainLife", "powerOf"),
            life("gainLife", "toughnessOf"),
        ],
    )
    .unwrap();
    assert_eq!(e.state.players[0].life, 29);
}

#[test]
fn graveyard_destination_replacement_still_commits_target_lki() {
    let (mut e, s, id) = position();
    e.state
        .rule_modifiers
        .push(json!({"kind":"exileIfLeavesBattlefield","cardInstanceId":id}));
    run(&mut e, &s, vec![destroy(), life("loseLife", "manaValueOf")]).unwrap();
    assert_eq!(e.state.players[0].life, 15);
    assert!(e.state.players[1].graveyard.is_empty());
    assert_eq!(e.state.players[1].exile.len(), 1);
}

#[test]
fn blink_during_sequence_reads_old_object_and_new_object_has_new_marker() {
    let (mut e, s, id) = position();
    let old = GameEngine::tracked_zone_identity(&e.state.players[1].battlefield[0])
        .unwrap()
        .to_string();
    run(
        &mut e,
        &s,
        vec![
            json!({"kind":"blinkPermanent","permanent":object()}),
            life("loseLife", "manaValueOf"),
        ],
    )
    .unwrap();
    assert_eq!(e.state.players[0].life, 15);
    let (p, i) = e.permanent_position(&id).unwrap();
    assert_ne!(
        GameEngine::tracked_zone_identity(&e.state.players[p].battlefield[i]),
        Some(old.as_str())
    );
}

#[test]
fn departed_pending_target_metadata_survives_the_stack_pop() {
    let (mut e, s, id) = position();
    e.state.stack.push(s);
    e.exile_permanent(&id).unwrap();
    let popped = e.pop_stack_object_with_lki().unwrap();
    let captured = GameEngine::captured_target_numeric_binding(&popped, "targetPermanent").unwrap();
    assert_eq!(captured.last_known.unwrap().mana_value, 5);
}

#[test]
fn blink_before_resolution_fizzles_instead_of_using_returned_target() {
    let (mut e, mut s, _) = position();
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[],
        "declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"targetPermanent","minimum":1,"maximum":1,
        "candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}}}]},
        "effects":[life("loseLife","manaValueOf")]});
    s.ability_kind = Some("activatedAbility".into());
    s.ability_rule = Some(rule);
    e.state.stack.push(s.clone());
    run(
        &mut e,
        &s,
        vec![json!({"kind":"blinkPermanent","permanent":object()})],
    )
    .unwrap();
    e.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(e.state.players[0].life, 20);
    assert!(
        e.state
            .events
            .iter()
            .any(|event| event.kind == "abilityFizzled")
    );
}

#[test]
fn retargeting_removes_old_snapshot_with_original_designation() {
    let (mut e, mut s, id) = position();
    e.state.stack.push(s.clone());
    e.exile_permanent(&id).unwrap();
    s = e.pop_stack_object_with_lki().unwrap();
    assert!(
        GameEngine::captured_target_numeric_binding(&s, "targetPermanent")
            .unwrap()
            .last_known
            .is_some()
    );
    let mut replacement = card_instance(
        "player-1",
        &definition("replacement", "Creature - Scout", "{3}"),
        3,
    );
    e.prepare_battlefield_entry_identity(&mut replacement);
    let reference = TargetRef::Permanent {
        instance_id: replacement.instance_id.clone(),
    };
    e.state.players[1].battlefield.push(replacement);
    s.targets
        .insert("targetPermanent".into(), reference.clone());
    e.capture_target_incarnation(&mut s.decisions, "targetPermanent", &reference)
        .expect("test target must have an admitted incarnation");
    let recaptured = GameEngine::captured_target_numeric_binding(&s, "targetPermanent").unwrap();
    assert!(recaptured.last_known.is_none());
    run(&mut e, &s, vec![life("loseLife", "manaValueOf")]).unwrap();
    assert_eq!(e.state.players[0].life, 17);
}

#[test]
fn simultaneous_destroy_preserves_lord_effect_in_later_target_last_known_power() {
    let (mut e, s, id) = position();
    let mut lord = card_instance(
        "player-1",
        &definition("lord", "Creature - Scout", "{2}"),
        4,
    );
    lord.definition.rules = vec![json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
        "modifiers":[{"kind":"modifyPowerToughness","objects":{"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},
        "where":{"kind":"cardTypeContains","value":"Creature"}},"power":{"kind":"integer","value":1},"toughness":{"kind":"integer","value":1}}]})];
    e.prepare_battlefield_entry_identity(&mut lord);
    let lord_id = lord.instance_id.clone();
    e.state.players[1].battlefield.push(lord);
    assert_eq!(e.current_power(&e.state.players[1].battlefield[0]), 3);
    let context = e.target_numeric_context(&s);
    e.with_target_numeric_context(context, |e| {
        e.destroy_permanent_collection_from_stack(vec![lord_id, id], false, &s)?;
        assert_eq!(
            e.read_chosen_target_numeric(TargetNumericProperty::Power, &s, "targetPermanent"),
            Ok(3)
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn scope_error_cleans_context_and_missing_quantity_never_changes_life() {
    let (mut e, s, _) = position();
    let result = run(
        &mut e,
        &s,
        vec![
            destroy(),
            json!({"kind":"loseLife","player":{"kind":"abilityController"},
        "amount":{"kind":"decisionResult","decisionId":"undeclared"}}),
        ],
    );
    assert!(result.is_err());
    assert_eq!(e.state.players[0].life, 20);
    assert!(
        e.target_numeric_contexts
            .binding(&s.id, "targetPermanent")
            .is_none()
    );
}

#[test]
fn missing_incarnation_is_not_repaired_and_zero_mana_value_remains_valid() {
    let (mut e, mut s, _) = position();
    GameEngine::clear_tracked_zone_identity(&mut e.state.players[1].battlefield[0]);
    let reference = s.targets["targetPermanent"].clone();
    let original_decisions = s.decisions.clone();
    assert!(
        e.capture_target_incarnation(&mut s.decisions, "targetPermanent", &reference)
            .is_err()
    );
    assert_eq!(
        s.decisions, original_decisions,
        "failed capture must not partially rewrite decisions"
    );
    // The writer now refuses null captures. Keep the independent reader test
    // using an explicitly malformed historical/wire capture, not that writer.
    let mut malformed = s.clone();
    malformed
        .decisions
        .get_mut("__deepdeckTargetBattlefieldIdentitiesV1")
        .unwrap()["targetPermanent"]["zoneIdentity"] = Value::Null;
    assert_eq!(
        e.read_chosen_target_numeric(
            TargetNumericProperty::ManaValue,
            &malformed,
            "targetPermanent"
        ),
        Err(TargetNumericReadError::MissingIncarnation)
    );
    assert!(run(&mut e, &malformed, vec![life("loseLife", "manaValueOf")]).is_err());
    assert!(GameEngine::tracked_zone_identity(&e.state.players[1].battlefield[0]).is_none());
    e.state.players[1].battlefield[0].definition.mana_cost = "{0}".into();
    let mut target = e.state.players[1].battlefield.remove(0);
    e.prepare_battlefield_entry_identity(&mut target);
    e.state.players[1].battlefield.push(target);
    e.capture_target_incarnation(&mut s.decisions, "targetPermanent", &reference)
        .expect("test target must have an admitted incarnation");
    run(&mut e, &s, vec![destroy(), life("loseLife", "manaValueOf")]).unwrap();
    assert_eq!(e.state.players[0].life, 20);
}

#[test]
fn engine_checkpoint_rolls_back_active_target_lki_after_a_departure_error() {
    let (mut e, s, id) = position();
    let context = e.target_numeric_context(&s);
    e.with_target_numeric_context(context, |e| {
        let result: Result<(), EngineError> = e.with_replacement_checkpoint(true, |e| {
            e.exile_permanent(&id)?;
            Err(EngineError::new(
                "injected failure after committed departure",
            ))
        });
        assert!(result.is_err());
        assert!(e.permanent_position(&id).is_some());
        assert!(
            e.target_numeric_contexts
                .binding(&s.id, "targetPermanent")
                .unwrap()
                .last_known
                .is_none()
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn state_only_cost_checkpoint_also_restores_active_target_lki() {
    let (mut e, s, id) = position();
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},
        "costs":[{"kind":"sacrificePermanent","permanent":{"kind":"self"}}],"effects":[]});
    let action = LegalAction {
        id: "test-cost".into(),
        kind: ActionKind::ActivateAbility,
        player_id: "player-1".into(),
        label: "Sacrifice source".into(),
        card_instance_id: Some(id.clone()),
        payment_sources: vec![],
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
        target_order: vec![],
        attacker_id: None,
        blocker_id: None,
    };
    let payment = e.prepare_activated_cost_payment(&action, &rule).unwrap();
    // Deliberately inject a destination owner lookup failure after a valid
    // preparation. This tests rollback consistency, not a valid-card scenario.
    e.state.players[1].battlefield[0].owner = "injected-invalid-owner".into();
    let context = e.target_numeric_context(&s);
    e.with_target_numeric_context(context, |e| {
        assert!(e.commit_activated_cost_payment(&payment).is_err());
        assert!(e.permanent_position(&id).is_some());
        assert!(
            e.target_numeric_contexts
                .binding(&s.id, "targetPermanent")
                .unwrap()
                .last_known
                .is_none()
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn negative_property_stays_signed_but_direct_life_results_are_nonnegative() {
    let (mut e, s, _) = position();
    let mut watcher = card_instance(
        "player-0",
        &definition("life-loss-watcher", "Enchantment", "{0}"),
        8,
    );
    watcher.definition.rules = vec![json!({"kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"lifeLost","player":{"kind":"controllerOf","object":{"kind":"self"}}},
        "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]})];
    e.prepare_battlefield_entry_identity(&mut watcher);
    e.state.players[0].battlefield.push(watcher);
    let mut negative = buff();
    negative["power"] = json!({"kind":"integer","value":-5});
    negative["toughness"] = json!({"kind":"integer","value":0});
    run(
        &mut e,
        &s,
        vec![
            negative,
            life("gainLife", "powerOf"),
            life("loseLife", "powerOf"),
        ],
    )
    .unwrap();
    assert_eq!(
        e.read_chosen_target_numeric(TargetNumericProperty::Power, &s, "targetPermanent"),
        Ok(-3)
    );
    assert_eq!(e.state.players[0].life, 20);
    assert!(
        !e.state
            .events
            .iter()
            .any(|event| event.kind == "lifeLost" || event.kind == "lifeGained")
    );
    assert!(
        e.state.stack.is_empty(),
        "zero life gain/loss cannot queue a life-change trigger"
    );
}

#[test]
fn initially_illegal_target_cannot_supply_lki_while_another_target_remains_legal() {
    let (mut e, mut s, id) = position();
    s.targets.insert(
        "legalPlayer".into(),
        TargetRef::Player {
            player_id: "player-0".into(),
        },
    );
    s.ability_kind = Some("activatedAbility".into());
    s.ability_rule = Some(
        json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[],
        "declaration":{"kind":"castingDeclaration","decisions":[
            {"kind":"chooseTargets","id":"targetPermanent","minimum":1,"maximum":1,"candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}}},
            {"kind":"chooseTargets","id":"legalPlayer","minimum":1,"maximum":1,"candidates":{"kind":"players"}}
        ]},"effects":[life("loseLife","manaValueOf"),{"kind":"gainLife","player":{"kind":"chosenTarget","id":"legalPlayer"},"amount":{"kind":"integer","value":1}},
            {"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"conditionalValue",
                "condition":{"kind":"compare","operator":"==","left":{"kind":"integer","value":1},"right":{"kind":"integer","value":1}},
                "ifTrue":{"kind":"integer","value":3},"ifFalse":numeric("manaValueOf")}}
        ]}),
    );
    e.state.stack.push(s);
    e.exile_permanent(&id).unwrap();
    e.resolve_top_stack(&mut NoChoices).unwrap();
    // No MV5 information can be read from the initially illegal permanent.
    // The independent legal-player effect still resolves normally.
    // An unused conditional branch does not require the illegal target's info.
    assert_eq!(e.state.players[0].life, 24);
}

#[test]
fn direct_child_can_read_authenticated_live_target_but_cannot_invent_a_departure_scope() {
    let (mut e, s, _) = position();
    e.execute_effect(
        &life("gainLife", "powerOf"),
        None,
        &s,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut NoChoices,
    )
    .unwrap();
    assert_eq!(e.state.players[0].life, 22);
    e.execute_effect(
        &destroy(),
        None,
        &s,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut NoChoices,
    )
    .unwrap();
    assert_eq!(
        e.read_chosen_target_numeric(TargetNumericProperty::ManaValue, &s, "targetPermanent"),
        Err(TargetNumericReadError::MissingResolutionContext)
    );
    assert!(
        e.execute_effect(
            &life("loseLife", "manaValueOf"),
            None,
            &s,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut NoChoices
        )
        .is_err()
    );
    assert_eq!(e.state.players[0].life, 22);
    assert!(!e.state.events.iter().any(|event| event.kind == "lifeLost"));
}

#[test]
fn scoped_adapter_installs_player_optional_and_unauthenticated_target_states() {
    let (mut engine, mut stack, _) = position();
    stack.targets.clear();
    stack.decisions.clear();
    stack.targets.insert(
        "player".into(),
        TargetRef::Player {
            player_id: "player-1".into(),
        },
    );
    let context = engine.target_numeric_context(&stack);
    assert!(context.targets.is_empty());
    engine
        .with_target_numeric_context(context, |engine| {
            assert_eq!(
                engine.target_was_legal_at_resolution_start(&stack, "player"),
                Some(true)
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        engine.target_was_legal_at_resolution_start(&stack, "player"),
        None
    );
    stack.targets.clear();
    stack.decisions.insert("player".into(), json!([]));
    stack.ability_rule = Some(
        json!({"kind":"spellAbility","source":{"kind":"self"},"declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"player","minimum":0,"maximum":1,"candidates":{"kind":"players"}}]},"effects":[]}),
    );
    let context = engine.target_numeric_context(&stack);
    engine
        .with_target_numeric_context(context, |engine| {
            assert_eq!(
                engine.target_was_legal_at_resolution_start(&stack, "player"),
                Some(false)
            );
            Ok(())
        })
        .unwrap();
    // Neither absent declaration capture nor a corrupt live marker is an
    // authenticated game-rules departure/illegal target.
    for missing_capture in [true, false] {
        let (mut engine, mut stack, id) = position();
        if missing_capture {
            stack.decisions.clear();
        } else {
            GameEngine::clear_tracked_zone_identity(&mut engine.state.players[1].battlefield[0]);
        }
        let context = engine.target_numeric_context(&stack);
        let called = std::cell::Cell::new(false);
        assert!(
            engine
                .with_target_numeric_context(context, |_| {
                    called.set(true);
                    Ok(())
                })
                .is_err()
        );
        assert!(!called.get());
        assert!(engine.permanent_position(&id).is_some());
        engine.state.stack.push(stack);
        let before = serde_json::to_value(&engine.state).unwrap();
        assert!(engine.resolve_top_stack(&mut NoChoices).is_err());
        assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    }
}

#[test]
fn illegal_target_predicate_is_unavailable_but_independent_union_survives() {
    use super::super::runtime_collections::RuntimeCollectionResolution;
    let (mut engine, stack, _) = position();
    engine
        .execute_effect(
            &destroy(),
            None,
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut NoChoices,
        )
        .unwrap();
    let candidate = card_instance("player-0", &definition("candidate", "Instant", "{0}"), 92);
    let id = candidate.instance_id.clone();
    engine.state.players[0].graveyard.push(candidate);
    let complete = json!({"kind":"cardsInZone","zone":{"kind":"graveyard","player":{"kind":"abilityController"}}});
    let dependent = json!({"kind":"filterObjects","objects":complete,"where":{"kind":"compare","left":{"kind":"manaValueOf","object":{"kind":"candidate"}},"operator":"<=","right":numeric("powerOf")}});
    let context = engine.target_numeric_context(&stack);
    engine
        .with_target_numeric_context(context, |engine| {
            assert!(matches!(
                engine.resolve_runtime_collection_status(
                    &dependent,
                    &stack,
                    &BTreeMap::new(),
                    &stack.decisions
                )?,
                RuntimeCollectionResolution::Unavailable
            ));
            let union = json!({"kind":"union","sets":[dependent,complete]});
            assert_eq!(
                engine.resolve_runtime_object_ids_for_effect(
                    &union,
                    &stack,
                    &BTreeMap::new(),
                    &stack.decisions
                )?,
                Some(vec![id])
            );
            assert!(engine.numeric_amount_requires_initially_illegal_target(
                &json!({"kind":"countObjects","objects":union}),
                &stack,
                &BTreeMap::new(),
                &stack.decisions
            )?);
            Ok(())
        })
        .unwrap();
}
