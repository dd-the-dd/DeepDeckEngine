use super::*;

struct NoChoices;
impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new("linked return needs no default choice"))
    }
}

struct EntryChoice {
    calls: usize,
    require_both_sources_absent: bool,
    require_mana_empty: bool,
}
impl DecisionProvider for EntryChoice {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        assert_eq!(request.player_id, "player-1");
        if self.require_mana_empty {
            assert!(
                state.players[0].mana_pool.is_empty(),
                "entry choice precedes immediate mana production"
            );
        }
        if self.require_both_sources_absent {
            assert!(
                state
                    .players
                    .iter()
                    .flat_map(|player| &player.battlefield)
                    .all(|card| card.instance_id != "source" && card.instance_id != "other")
            );
        }
        self.calls += 1;
        Ok(1)
    }
}

fn definition(id: &str, kind: &str) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":id,"typeLine":kind,"manaCost":"{1}","power":"2","toughness":"2"}),
    )
    .unwrap()
}

fn card(id: &str, kind: &str, owner: &str, controller: &str) -> CardInstance {
    let mut result = card_instance(owner, &definition(id, kind), 0);
    result.instance_id = id.to_string();
    result.controller = controller.to_string();
    result
}

fn setup() -> (GameEngine, StackObject) {
    let filler = definition("filler", "Basic Land — Plains");
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player{index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        918,
    )
    .unwrap();
    let mut source = card("source", "Artifact", "player-0", "player-0");
    let mut captive = card("captive", "Creature", "player-1", "player-0");
    engine.prepare_battlefield_entry_identity(&mut source);
    engine.prepare_battlefield_entry_identity(&mut captive);
    engine.state.players[0]
        .battlefield
        .extend([source.clone(), captive]);
    let stack = StackObject {
        id: "stack:linked-exile".into(),
        controller: "player-0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "victim".into(),
            TargetRef::Permanent {
                instance_id: "captive".into(),
            },
        )]),
    };
    (engine, stack)
}

fn effects(
    engine: &mut GameEngine,
    stack: &StackObject,
    effects: Value,
    provider: &mut impl DecisionProvider,
) -> Result<(), EngineError> {
    engine.execute_effects(
        &effects,
        stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        provider,
    )
}

fn exile(engine: &mut GameEngine, stack: &StackObject) {
    let (player, index) = engine.permanent_position("captive").unwrap();
    // These setup rules describe printed abilities before the first movement.
    let captive = &mut engine.state.players[player].battlefield[index];
    captive.printed_definition = Some(captive.definition.clone());
    let last_known_definition = captive.definition.clone();
    // Private fixture preparation proves the existing lifecycle service only.
    // The retired Oracle transport is not executed or claimed covered here.
    engine.exile_permanent("captive").unwrap();
    engine
        .register_linked_exiled_permanent(&stack.card, "captive", true, &last_known_definition)
        .unwrap();
    assert!(engine.permanent_position("captive").is_none());
    assert_eq!(engine.state.players[1].exile[0].instance_id, "captive");
}

fn color_entry() -> Value {
    json!({
        "kind":"replacementEffect","source":{"kind":"self"},
        "event":{"kind":"wouldEnterBattlefield","object":{"kind":"self"}},
        "decisions":[{"kind":"chooseColor","id":"returnColor","options":["R","G"]}],
        "replacement":[{"kind":"storeDecision","decisionId":"returnColor"}],
    })
}

#[test]
fn linked_return_is_immediate_nonstack_and_uses_the_physical_owner() {
    let (mut engine, stack) = setup();
    engine.state.players[0].battlefield[1].definition.rules.push(json!({
        "kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"enterBattlefield","object":{"kind":"self"}},
        "effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"integer","value":1}}],
    }));
    exile(&mut engine, &stack);
    effects(
        &mut engine,
        &stack,
        json!([{"kind":"returnToOwnersHand","object":{"kind":"self"}}]),
        &mut NoChoices,
    )
    .unwrap();
    let (player, index) = engine.permanent_position("captive").unwrap();
    assert_eq!(player, 1);
    assert_eq!(
        engine.state.players[player].battlefield[index].controller,
        "player-1"
    );
    assert!(!engine.has_pending_linked_exile_returns());
    assert_eq!(
        engine.state.stack.len(),
        1,
        "only the returned object's real entry trigger is on the stack"
    );
    assert_eq!(engine.state.stack[0].card.instance_id, "captive");
    engine.resolve_top_stack(&mut NoChoices).unwrap();
    assert_eq!(engine.state.players[1].life, 21);
    assert!(engine.state.stack.is_empty());
}

#[test]
fn all_source_departure_destinations_queue_the_same_immediate_transition() {
    for destination in ["exile", "hand", "library", "graveyard"] {
        let (mut engine, stack) = setup();
        exile(&mut engine, &stack);
        match destination {
            "exile" => engine.exile_permanent("source").unwrap(),
            "hand" => engine.return_permanent_to_hand("source").unwrap(),
            "library" => engine
                .put_permanent_on_owner_library("source", "top")
                .unwrap(),
            _ => engine
                .move_permanent_to_graveyard("source", "test")
                .unwrap(),
        }
        assert!(
            engine.state.stack.is_empty(),
            "{destination}: no synthetic return trigger"
        );
        assert!(engine.has_pending_linked_exile_returns());
        engine
            .check_state_based_actions_with_provider(Some(&mut NoChoices))
            .unwrap();
        assert!(
            engine.permanent_position("captive").is_some(),
            "{destination}"
        );
    }
}

#[test]
fn departed_or_reentered_source_fails_the_same_battlefield_identity_check() {
    for returns in [false, true] {
        let (mut engine, stack) = setup();
        assert!(engine.linked_source_is_same_battlefield_object(&stack.card));
        engine.return_permanent_to_hand("source").unwrap();
        if returns {
            let mut returned = GameEngine::take_card_from_player_named_zone(
                &mut engine.state.players[0],
                "hand",
                "source",
            )
            .unwrap();
            returned.controller = "player-0".into();
            engine.prepare_battlefield_entry_identity(&mut returned);
            engine.state.players[0].battlefield.push(returned.clone());
            engine.enqueue_enter_battlefield_triggers(&returned);
        }
        // An identity-service test, not an old pending-ability execution proof.
        assert!(!engine.linked_source_is_same_battlefield_object(&stack.card));
        assert!(engine.permanent_position("captive").is_some());
        assert!(engine.state.players[1].exile.is_empty());
    }
}

#[test]
fn a_different_exile_incarnation_does_not_return_with_the_old_link() {
    let (mut engine, stack) = setup();
    exile(&mut engine, &stack);
    let former = GameEngine::take_card_from_player_named_zone(
        &mut engine.state.players[1],
        "exile",
        "captive",
    )
    .unwrap();
    assert!(GameEngine::tracked_zone_identity(&former).is_none());
    engine.state.players[1].hand.push(former);
    let reexiled = GameEngine::take_card_from_player_named_zone(
        &mut engine.state.players[1],
        "hand",
        "captive",
    )
    .unwrap();
    engine.state.players[1].exile.push(reexiled);
    engine.return_permanent_to_hand("source").unwrap();
    engine.drain_linked_exile_returns(&mut NoChoices).unwrap();
    assert!(engine.permanent_position("captive").is_none());
    assert_eq!(engine.state.players[1].exile.len(), 1);
}

#[test]
fn simultaneous_destruction_finishes_its_batch_before_any_return_entry_choice() {
    let (mut engine, stack) = setup();
    engine.state.players[0].battlefield[1]
        .definition
        .rules
        .push(color_entry());
    engine.state.players[0]
        .battlefield
        .push(card("other", "Artifact", "player-0", "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture construction precedes references");
    exile(&mut engine, &stack);
    let destroyed = engine
        .destroy_permanent_collection_from_stack(
            vec!["source".into(), "other".into()],
            false,
            &stack,
        )
        .unwrap();
    assert_eq!(destroyed.len(), 2);
    assert!(engine.permanent_position("captive").is_none());
    assert!(engine.state.stack.is_empty());
    let mut provider = EntryChoice {
        calls: 0,
        require_both_sources_absent: true,
        require_mana_empty: false,
    };
    engine.drain_linked_exile_returns(&mut provider).unwrap();
    assert_eq!(provider.calls, 1);
    let (player, index) = engine.permanent_position("captive").unwrap();
    assert_eq!(
        stored_decision_value(
            &engine.state.players[player].battlefield[index].definition,
            "returnColor"
        ),
        Some("G")
    );
}

#[test]
fn queued_returns_and_zone_markers_participate_in_state_and_choice_rollback() {
    let (mut engine, stack) = setup();
    engine.state.players[0].battlefield[1]
        .definition
        .rules
        .push(color_entry());
    exile(&mut engine, &stack);
    let before_payment = engine.state.clone();
    engine.return_permanent_to_hand("source").unwrap();
    assert!(engine.has_pending_linked_exile_returns());
    engine.refresh_control_effects();
    assert!(
        engine.has_pending_linked_exile_returns(),
        "source absence cleanup preserves private continuations"
    );
    engine.state = before_payment.clone();
    assert!(!engine.has_pending_linked_exile_returns());
    assert!(engine.permanent_position("source").is_some());
    engine.return_permanent_to_hand("source").unwrap();
    let pending = engine.state.clone();
    assert!(engine.drain_linked_exile_returns(&mut NoChoices).is_err());
    assert_eq!(
        engine.state, pending,
        "failed real entry choice restores the complete pending transition"
    );
    let mut provider = EntryChoice {
        calls: 0,
        require_both_sources_absent: false,
        require_mana_empty: false,
    };
    engine.drain_linked_exile_returns(&mut provider).unwrap();
    assert_eq!(provider.calls, 1);
    assert!(engine.permanent_position("captive").is_some());
}

#[test]
fn private_continuations_are_not_ir_and_historical_links_are_not_guessed() {
    assert!(effect_supported(&GameEngine::linked_return_movement()));
    let pending = json!({"kind":PENDING_KIND,"payload":{"state":"ready","entries":[]}});
    assert!(!effect_supported(&pending));
    assert!(!battlefield_static_modifier_supported(&pending));
    assert!(!rule_is_executable(
        &json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[pending]})
    ));
    let (mut engine, _) = setup();
    engine.exile_permanent("captive").unwrap();
    engine.state.rule_modifiers.push(json!({"kind":"exiledWithSource","sourceCardInstanceId":"source","cardInstanceId":"captive","returnWhenSourceLeaves":true}));
    engine.return_permanent_to_hand("source").unwrap();
    let error = engine
        .drain_linked_exile_returns(&mut NoChoices)
        .unwrap_err();
    assert!(error.to_string().contains("authenticated zone incarnation"));
}

#[test]
fn a_failed_return_choice_restores_the_paid_cast_and_a_returned_watcher_sees_the_cast() {
    let (mut engine, stack) = setup();
    engine.state.step = GameStep::PrecombatMain;
    let captive = &mut engine.state.players[0].battlefield[1];
    captive.definition.rules.push(color_entry());
    captive.definition.rules.push(json!({
        "kind":"triggeredAbility","source":{"kind":"self"},
        "event":{"kind":"spellCast","opponentOfSourceController":true,"where":null},
        "effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"integer","value":1}}]
    }));
    exile(&mut engine, &stack);
    let mut spell = card("cost-spell", "Sorcery", "player-0", "player-0");
    spell.definition.mana_cost = "{0}".into();
    spell.definition.rules = vec![json!({
        "kind":"spellAbility","source":{"kind":"self"},
        "declaration":{"kind":"castingDeclaration","decisions":[{
            "kind":"chooseObjects","id":"sacrificeCost",
            "candidates":{"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Artifact"}},
            "quantity":{"kind":"exactly","value":{"kind":"integer","value":1}}
        }],"additionalCosts":[{"kind":"sacrificePermanent","permanent":{"kind":"chosenObject","id":"sacrificeCost"}}]},
        "effects":[{"kind":"gainLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":{"kind":"integer","value":0}}]
    })];
    spell.printed_definition = Some(spell.definition.clone());
    // New fixture carrier is admitted at publication, before any cast capture.
    engine.admit_new_card_carrier(&mut spell).unwrap();
    engine.state.players[0].hand.push(spell);
    let cast = CanonicalSpellCast {
        source_zone: None,
        controller_id: "player-0".into(),
        source_definition_id: "cost-spell".into(),
        use_alternative_cost: false,
        alternative_exile_definition_id: None,
        declaration_decisions: BTreeMap::from([(
            "sacrificeCost".into(),
            json!([{"permanent":{"instanceId":"source"}}]),
        )]),
    };
    assert!(engine.canonical_spell_cast_available(&cast).unwrap());
    let before = engine.state.clone();
    let error = engine
        .cast_canonical_spell(&cast, &mut NoChoices)
        .unwrap_err();
    assert!(error.to_string().contains("default choice"));
    assert_eq!(
        engine.state, before,
        "cost, source card, stack and pending queue all roll back"
    );
    let mut provider = EntryChoice {
        calls: 0,
        require_both_sources_absent: false,
        require_mana_empty: true,
    };
    engine.cast_canonical_spell(&cast, &mut provider).unwrap();
    assert_eq!(provider.calls, 1);
    let entered = engine
        .state
        .events
        .iter()
        .position(|event| {
            event.kind == "permanentEnteredBattlefield"
                && event.card_instance_id.as_deref() == Some("captive")
        })
        .unwrap();
    let cast_event = engine
        .state
        .events
        .iter()
        .position(|event| event.kind == "spellCast")
        .unwrap();
    assert!(entered < cast_event);
    assert_eq!(
        engine.state.stack.len(),
        2,
        "the returned watcher's real cast trigger exists above the spell"
    );
    assert_eq!(
        engine.state.stack.last().unwrap().card.instance_id,
        "captive"
    );
}

#[test]
fn a_mana_cost_return_completes_with_the_real_provider_before_production() {
    let (mut engine, stack) = setup();
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield[1]
        .definition
        .rules
        .push(color_entry());
    let source = &mut engine.state.players[0].battlefield[0];
    source.definition.rules = vec![json!({
        "kind":"manaAbility","source":{"kind":"self"},
        "costs":[{"kind":"sacrificePermanent","permanent":{"kind":"self"}}],
        "effects":[{"kind":"addMana","player":{"kind":"controllerOf","object":{"kind":"self"}},"mana":"{G}"}]
    })];
    source.printed_definition = Some(source.definition.clone());
    exile(&mut engine, &stack);
    let activation = CanonicalManaAbilityActivation {
        controller_id: "player-0".into(),
        source_definition_id: "source".into(),
        mana_symbols: Some(vec!["G".into()]),
    };
    let before = engine.state.clone();
    let declarations = engine.complete_checked_mana_activation_actions(0).unwrap();
    let deferred = declarations
        .iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("source")
                && action.decisions["manaProductionPreview"]["kind"].as_str()
                    == Some("deferredNeedsProvider")
        })
        .expect("real entry decision preserves an unresolved explicit declaration");
    assert_eq!(
        deferred.decisions["manaProductionPreview"]["certified"],
        json!(false)
    );
    assert_eq!(deferred.decisions["manaDeclaredOutput"], json!(["G"]));
    let source = engine.state.players[0]
        .battlefield
        .iter()
        .find(|card| card.instance_id == "source")
        .unwrap();
    let choice = engine
        .try_checked_mana_activation_plan(source, None, true, true)
        .unwrap()
        .into_activation_choices()
        .into_iter()
        .next()
        .unwrap();
    assert!(matches!(
        engine
            .postcost_choices_for_declared_action(deferred, &choice)
            .unwrap(),
        super::super::mana_postcost_preview::PostcostManaPreview::DeferredNeedsProvider { .. }
    ));
    let automatic_error = engine
        .try_checked_payment_for_mana_costs_with_context(
            0,
            &["{G}".into()],
            &BTreeSet::new(),
            None,
            None,
            None,
        )
        .unwrap_err();
    assert!(
        automatic_error
            .message
            .contains("unresolved provider decision")
    );
    assert_eq!(
        engine.state, before,
        "both explicit and automatic previews are read only"
    );
    assert!(
        engine
            .activate_canonical_mana_ability(&activation, &mut NoChoices)
            .is_err()
    );
    assert_eq!(engine.state, before);
    let mut provider = EntryChoice {
        calls: 0,
        require_both_sources_absent: false,
        require_mana_empty: true,
    };
    engine
        .activate_canonical_mana_ability(&activation, &mut provider)
        .unwrap();
    assert_eq!(provider.calls, 1);
    assert_eq!(
        engine.state.players[0]
            .mana_pool
            .iter()
            .map(|mana| mana.symbol.as_str())
            .collect::<Vec<_>>(),
        ["G"]
    );
    assert!(engine.permanent_position("captive").is_some());
    assert!(engine.state.stack.is_empty());
}

#[test]
fn exiled_tokens_cannot_return_immediately_or_move_again_through_the_generic_primitive() {
    for linked in [false, true] {
        let (mut engine, stack) = setup();
        let token = &mut engine.state.players[0].battlefield[1];
        token.definition.is_token = true;
        token.printed_definition = Some(token.definition.clone());
        if linked {
            exile(&mut engine, &stack);
            effects(
                &mut engine,
                &stack,
                json!([{"kind":"returnToOwnersHand","object":{"kind":"self"}}]),
                &mut NoChoices,
            )
            .unwrap();
        } else {
            engine.exile_permanent("captive").unwrap();
            engine
                .execute_effects(
                    &json!([{
                        "kind":"moveCards","cards":{"kind":"boundObjects","binding":"tokenObjects"},
                        "to":{"kind":"hand","player":{"kind":"abilityController"}}
                    }]),
                    &stack,
                    &mut BTreeMap::from([(
                        "tokenObjects".into(),
                        RuntimeBinding::Objects(vec!["captive".into()]),
                    )]),
                    &mut BTreeMap::new(),
                    &mut NoChoices,
                )
                .unwrap();
        }
        assert!(engine.permanent_position("captive").is_none());
        assert!(
            engine
                .state
                .players
                .iter()
                .flat_map(|player| &player.hand)
                .all(|card| card.instance_id != "captive")
        );
        assert_eq!(
            engine.state.players[1].exile.len(),
            1,
            "the token waits for SBA, it never changes zone again"
        );
        assert_eq!(
            engine
                .state
                .events
                .iter()
                .filter(|event| event.kind == "permanentEnteredBattlefield"
                    && event.card_instance_id.as_deref() == Some("captive"))
                .count(),
            0
        );
        engine
            .check_state_based_actions_with_provider(Some(&mut NoChoices))
            .unwrap();
        assert!(engine.state.players[1].exile.is_empty());
    }
}
