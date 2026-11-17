use super::super::numeric_evaluation::{IntegerRounding, checked_integer_arithmetic};
use super::super::*;
use super::{
    QuantityEvent, QuantityExpression, QuantityOrigin, QuantityRecipient, QuantityResolution,
    quantity_modifier_supported,
};

struct PickSource {
    first: String,
    expected_player: String,
    calls: usize,
}
impl DecisionProvider for PickSource {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        assert_eq!(request.player_id, self.expected_player);
        self.calls += 1;
        Ok(request
            .options
            .iter()
            .position(|action| action.card_instance_id.as_deref() == Some(self.first.as_str()))
            .unwrap_or(0))
    }
}

struct RefuseChoices;
impl DecisionProvider for RefuseChoices {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Err(EngineError::new("no implicit replacement choice"))
    }
}

fn definition(id: &str, types: &str) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":id,"typeLine":types,"manaCost":"{1}","power":"2","toughness":"2"}),
    )
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
                    cards: vec![definition("filler", "Basic Land — Island"); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        633,
    )
    .unwrap()
}

fn modifier(kind: &str, origin: &str, expression: Value) -> Value {
    let mut event = json!({"kind":kind,"origin":origin,"minimum":1,"player":{"kind":"controllerOf","object":{"kind":"self"}}});
    if kind == "counterPlacement" {
        event["recipientKind"] = json!("permanent");
    }
    json!({"kind":"replaceEventQuantity","event":event,"quantity":expression})
}

fn counter_addition() -> Value {
    let mut value = modifier(
        "counterPlacement",
        "any",
        json!({"kind":"add","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":1}}),
    );
    value["event"]["where"] = json!({"kind":"cardTypeContains","value":"Creature"});
    value["event"]["counter"] = json!("+1/+1");
    value
}

fn counter_doubling() -> Value {
    modifier(
        "counterPlacement",
        "effect",
        json!({"kind":"multiply","left":{"kind":"integer","value":2},"right":{"kind":"eventQuantity"}}),
    )
}

fn install(engine: &mut GameEngine, id: &str, modifiers: Vec<Value>) {
    let mut card = card_instance("player-1", &definition(id, "Enchantment"), 90);
    card.instance_id = id.to_string();
    card.controller = "player-0".to_string();
    card.definition.rules = vec![
        json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":modifiers}),
    ];
    engine.state.players[0].battlefield.push(card);
}

fn recipient() -> CardInstance {
    let mut card = card_instance("player-1", &definition("recipient", "Creature"), 2);
    card.controller = "player-0".to_string();
    card
}

#[test]
fn zero_quantity_never_transforms_or_requests_an_order() {
    let mut engine = engine();
    install(&mut engine, "addition", vec![counter_addition()]);
    install(&mut engine, "doubling", vec![counter_doubling()]);
    let card = recipient();
    let result = engine
        .resolve_quantity_event(
            QuantityEvent {
                amount: 0,
                origin: QuantityOrigin::Effect,
                recipient: QuantityRecipient::Permanent {
                    permanent: &card,
                    counter: "+1/+1",
                },
            },
            &mut RefuseChoices,
        )
        .unwrap();
    assert_eq!(
        result,
        QuantityResolution {
            amount: 0,
            original_origin: QuantityOrigin::Effect,
            effective_origin: QuantityOrigin::Effect,
            applied: vec![]
        }
    );
}

#[test]
fn affected_controller_selects_each_replacement_once_in_either_order() {
    let mut engine = engine();
    install(&mut engine, "addition", vec![counter_addition()]);
    install(&mut engine, "doubling", vec![counter_doubling()]);
    let card = recipient();
    for (first, expected) in [("addition", 4), ("doubling", 3)] {
        let mut provider = PickSource {
            first: first.to_string(),
            expected_player: "player-0".to_string(),
            calls: 0,
        };
        let result = engine
            .resolve_quantity_event(
                QuantityEvent {
                    amount: 1,
                    origin: QuantityOrigin::Effect,
                    recipient: QuantityRecipient::Permanent {
                        permanent: &card,
                        counter: "+1/+1",
                    },
                },
                &mut provider,
            )
            .unwrap();
        assert_eq!(result.amount, expected);
        assert_eq!(result.applied.len(), 2);
        assert_ne!(result.applied[0], result.applied[1]);
        assert_eq!(provider.calls, 1);
    }
}

#[test]
fn origin_counter_name_controller_and_subject_filters_remain_independent() {
    let mut engine = engine();
    install(&mut engine, "addition", vec![counter_addition()]);
    install(&mut engine, "doubling", vec![counter_doubling()]);
    let mut card = recipient();
    for origin in [QuantityOrigin::Cost, QuantityOrigin::RuleAction] {
        let result = engine
            .resolve_quantity_event(
                QuantityEvent {
                    amount: 1,
                    origin,
                    recipient: QuantityRecipient::Permanent {
                        permanent: &card,
                        counter: "+1/+1",
                    },
                },
                &mut RefuseChoices,
            )
            .unwrap();
        assert_eq!(result.amount, 4);
        assert_eq!(result.applied.len(), 2);
        assert_eq!(result.original_origin, origin);
        assert_eq!(result.effective_origin, QuantityOrigin::Effect);
    }
    card.definition.type_line = "Artifact".to_string();
    let result = engine
        .resolve_quantity_event(
            QuantityEvent {
                amount: 2,
                origin: QuantityOrigin::Effect,
                recipient: QuantityRecipient::Permanent {
                    permanent: &card,
                    counter: "Study",
                },
            },
            &mut RefuseChoices,
        )
        .unwrap();
    assert_eq!(result.amount, 4);
    assert_eq!(result.applied.len(), 1);
    card.controller = "player-1".to_string();
    let result = engine
        .resolve_quantity_event(
            QuantityEvent {
                amount: 2,
                origin: QuantityOrigin::Effect,
                recipient: QuantityRecipient::Permanent {
                    permanent: &card,
                    counter: "+1/+1",
                },
            },
            &mut RefuseChoices,
        )
        .unwrap();
    assert_eq!(result.amount, 2);
    assert!(result.applied.is_empty());
}

#[test]
fn duplicate_modifiers_are_distinct_programs_and_refused_choice_is_not_a_default() {
    let mut engine = engine();
    install(
        &mut engine,
        "two-additions",
        vec![counter_addition(), counter_addition()],
    );
    let card = recipient();
    let event = QuantityEvent {
        amount: 1,
        origin: QuantityOrigin::Effect,
        recipient: QuantityRecipient::Permanent {
            permanent: &card,
            counter: "+1/+1",
        },
    };
    let prior = engine.branching_decision_sequence.get();
    assert!(
        engine
            .resolve_quantity_event(event, &mut RefuseChoices)
            .is_err()
    );
    assert_eq!(engine.branching_decision_sequence.get(), prior);
    let mut provider = PickSource {
        first: "two-additions".to_string(),
        expected_player: "player-0".to_string(),
        calls: 0,
    };
    let result = engine.resolve_quantity_event(event, &mut provider).unwrap();
    assert_eq!(result.amount, 3);
    assert_eq!(result.applied.len(), 2);
}

#[test]
fn token_events_use_creator_and_template_without_exposing_quantity_to_other_contexts() {
    let mut engine = engine();
    install(
        &mut engine,
        "token-quantity",
        vec![modifier(
            "tokenCreation",
            "effect",
            json!({"kind":"multiply","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":2}}),
        )],
    );
    let template = definition("token", "Artifact");
    let result = engine
        .resolve_quantity_event(
            QuantityEvent {
                amount: 3,
                origin: QuantityOrigin::Effect,
                recipient: QuantityRecipient::Token {
                    controller: "player-0",
                    definition: &template,
                },
            },
            &mut RefuseChoices,
        )
        .unwrap();
    assert_eq!(result.amount, 6);
    assert!(!numeric_expression_supported(
        &json!({"kind":"eventQuantity"})
    ));
    let mut external = counter_doubling();
    external["quantity"] = json!({"kind":"decisionResult","decisionId":"xValue"});
    assert_eq!(quantity_modifier_supported(&external), Some(false));
    external = counter_doubling();
    external["event"]["recipientKind"] = json!("player");
    assert_eq!(quantity_modifier_supported(&external), Some(false));
}

#[test]
fn scoped_arithmetic_preserves_rounding_and_surfaces_invalid_operations() {
    for (round, expected) in [(IntegerRounding::Down, 1), (IntegerRounding::Up, 2)] {
        let expression = QuantityExpression::Divide {
            left: Box::new(QuantityExpression::EventQuantity),
            right: Box::new(QuantityExpression::Integer { value: 2 }),
            round,
        };
        assert_eq!(expression.evaluate(3).unwrap(), expected);
    }
    let expression: QuantityExpression = serde_json::from_value(json!({"kind":"divide","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":0},"round":"down"})).unwrap();
    assert!(expression.evaluate(3).is_err());
    assert!(checked_integer_arithmetic("multiply", i32::MAX, 2, None).is_none());
    assert!(serde_json::from_value::<QuantityExpression>(json!({"kind":"divide","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":2}})).is_err());
}

#[test]
fn counter_identifiers_are_opaque_and_case_is_not_normalized() {
    let mut engine = engine();
    let mut replacement = counter_addition();
    replacement["event"]["counter"] = json!("Study");
    install(&mut engine, "opaque-counter", vec![replacement]);
    let card = recipient();
    for (name, expected) in [("Study", 2), ("study", 1), ("another-name", 1)] {
        let result = engine
            .resolve_quantity_event(
                QuantityEvent {
                    amount: 1,
                    origin: QuantityOrigin::Effect,
                    recipient: QuantityRecipient::Permanent {
                        permanent: &card,
                        counter: name,
                    },
                },
                &mut RefuseChoices,
            )
            .unwrap();
        assert_eq!(result.amount, expected);
    }
}

#[test]
fn effect_only_programs_ignore_unmodified_costs_and_rule_actions() {
    let mut engine = engine();
    install(&mut engine, "effect-only", vec![counter_doubling()]);
    let card = recipient();
    for origin in [QuantityOrigin::Cost, QuantityOrigin::RuleAction] {
        let result = engine
            .resolve_quantity_event(
                QuantityEvent {
                    amount: 2,
                    origin,
                    recipient: QuantityRecipient::Permanent {
                        permanent: &card,
                        counter: "loyalty",
                    },
                },
                &mut RefuseChoices,
            )
            .unwrap();
        assert_eq!(result.amount, 2);
        assert!(result.applied.is_empty());
        assert_eq!(result.original_origin, origin);
        assert_eq!(result.effective_origin, origin);
    }
}

#[test]
fn applicability_is_rechecked_after_a_quantity_decrease() {
    let mut engine = engine();
    let subtraction = modifier(
        "counterPlacement",
        "any",
        json!({"kind":"subtract","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":1}}),
    );
    let mut threshold = counter_doubling();
    threshold["event"]["minimum"] = json!(2);
    install(&mut engine, "subtraction", vec![subtraction]);
    install(&mut engine, "threshold", vec![threshold]);
    let card = recipient();
    let mut provider = PickSource {
        first: "subtraction".to_string(),
        expected_player: "player-0".to_string(),
        calls: 0,
    };
    let result = engine
        .resolve_quantity_event(
            QuantityEvent {
                amount: 2,
                origin: QuantityOrigin::Effect,
                recipient: QuantityRecipient::Permanent {
                    permanent: &card,
                    counter: "charge",
                },
            },
            &mut provider,
        )
        .unwrap();
    assert_eq!(result.amount, 1);
    assert_eq!(result.applied.len(), 1);
    assert_eq!(provider.calls, 1);
}

#[test]
fn a_departed_replacement_source_is_not_borrowed_from_a_snapshot() {
    let mut engine = engine();
    install(&mut engine, "departing-source", vec![counter_doubling()]);
    let source_snapshot = engine.state.players[0].battlefield.pop().unwrap();
    engine.state.players[1].graveyard.push(source_snapshot);
    let card = recipient();
    let result = engine
        .resolve_quantity_event(
            QuantityEvent {
                amount: 1,
                origin: QuantityOrigin::Effect,
                recipient: QuantityRecipient::Permanent {
                    permanent: &card,
                    counter: "charge",
                },
            },
            &mut RefuseChoices,
        )
        .unwrap();
    assert_eq!(result.amount, 1);
    assert!(result.applied.is_empty());
}

#[test]
fn entry_counter_fragments_are_one_contained_event_before_batch_commit() {
    let mut engine = engine();
    install(&mut engine, "doubling", vec![counter_doubling()]);
    let mut entrant = recipient();
    entrant.definition.rules = vec![
        json!({"kind":"replacementEffect","source":{"kind":"self"},"event":{"kind":"wouldEnterBattlefield","object":{"kind":"self"}},"replacement":[{"kind":"putEnteringCounters","counter":"charge","count":{"kind":"integer","value":1}}]}),
    ];
    entrant.counters.insert("charge".to_string(), 1);
    engine
        .apply_enter_replacements(&mut entrant, &mut RefuseChoices)
        .unwrap();
    assert_eq!(entrant.counters.get("charge"), Some(&4));
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|p| p.instance_id != entrant.instance_id)
    );
    let id = entrant.instance_id.clone();
    engine.commit_prepared_battlefield_entries(vec![(0, entrant)]);
    let events = engine
        .state
        .events
        .iter()
        .filter(|e| {
            e.kind == "countersPlaced" && e.card_instance_id.as_deref() == Some(id.as_str())
        })
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].detail["count"], json!(4));
}

#[test]
fn intrinsic_entry_loyalty_and_lore_are_effects_but_main_phase_lore_is_rule_action() {
    let mut engine = engine();
    install(&mut engine, "doubling", vec![counter_doubling()]);
    let mut planeswalker = card_instance("player-0", &definition("walker", "Planeswalker"), 301);
    planeswalker.definition.rules = vec![
        json!({"kind":"activatedAbility","source":{"kind":"self"},"startingLoyalty":{"kind":"integer","value":3},"costs":[{"kind":"payLoyalty","object":{"kind":"self"},"amount":{"kind":"integer","value":1}}],"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]}),
    ];
    engine
        .apply_enter_replacements(&mut planeswalker, &mut RefuseChoices)
        .unwrap();
    assert_eq!(planeswalker.counters.get("loyalty"), Some(&6));
    let mut saga = card_instance("player-0", &definition("saga", "Enchantment - Saga"), 302);
    saga.definition.rules=(1..=4).map(|chapter|json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"sagaChapterReached","object":{"kind":"self"},"chapters":[{"kind":"integer","value":chapter}]},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]})).collect();
    engine
        .apply_enter_replacements(&mut saga, &mut RefuseChoices)
        .unwrap();
    assert_eq!(saga.counters.get("lore"), Some(&2));
    let id = saga.instance_id.clone();
    engine.commit_prepared_battlefield_entries(vec![(0, saga)]);
    assert_eq!(engine.state.stack.len(), 2);
    engine.state.stack.clear();
    engine
        .advance_active_player_sagas(&mut RefuseChoices)
        .unwrap();
    let (p, c) = engine.permanent_position(&id).unwrap();
    assert_eq!(
        engine.state.players[p].battlefield[c].counters.get("lore"),
        Some(&3)
    );
    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn token_outer_doubling_precedes_independent_riot_and_counter_events() {
    let mut engine = engine();
    let token_multiplier = modifier(
        "tokenCreation",
        "effect",
        json!({"kind":"multiply","left":{"kind":"integer","value":2},"right":{"kind":"eventQuantity"}}),
    );
    install(
        &mut engine,
        "season",
        vec![token_multiplier, counter_doubling()],
    );
    install(
        &mut engine,
        "riot",
        vec![
            json!({"kind":"grantKeyword","objects":{"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Creature"}},"keyword":"riot"}),
        ],
    );
    let mut executor = card_instance("player-0", &definition("executor", "Artifact"), 303);
    executor.definition.id = "executor".to_string();
    engine.state.players[0].battlefield.push(executor);
    let mut provider = PickSource {
        first: "unused".to_string(),
        expected_player: "player-0".to_string(),
        calls: 0,
    };
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"createTokens","controller":{"kind":"abilityController"},"quantity":{"kind":"integer","value":1},"token":{"name":"Witness","types":["Creature"],"power":{"kind":"integer","value":1},"toughness":{"kind":"integer","value":1}}}]});
    engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "player-0".to_string(),
                source_definition_id: "executor".to_string(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut provider,
        )
        .unwrap();
    let tokens = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|p| p.definition.is_token)
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 2);
    assert!(tokens.iter().all(|p| p.counters.get("+1/+1") == Some(&2)));
    assert_eq!(provider.calls, 2);
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|e| e.kind == "countersPlaced")
            .count(),
        2
    );
}

#[test]
fn zero_modified_counter_placement_still_pays_a_legal_cost() {
    let mut engine = engine();
    install(
        &mut engine,
        "subtract",
        vec![modifier(
            "counterPlacement",
            "any",
            json!({"kind":"subtract","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":1}}),
        )],
    );
    let mut card = recipient();
    card.instance_id = "cost-source".to_string();
    engine.state.players[0].battlefield.push(card);
    engine
        .pay_counter_placement_cost("cost-source", "charge", 1, "player-0", &mut RefuseChoices)
        .unwrap();
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .last()
            .unwrap()
            .counters
            .get("charge"),
        None
    );
    assert!(
        engine
            .state
            .events
            .iter()
            .all(|e| e.kind != "countersPlaced")
    );
}

#[test]
fn counter_cost_order_refusal_restores_mana_life_tap_flags_and_stack() {
    let mut engine = engine();
    install(&mut engine, "addition", vec![counter_addition()]);
    install(
        &mut engine,
        "multiply-any",
        vec![modifier(
            "counterPlacement",
            "any",
            json!({"kind":"multiply","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":2}}),
        )],
    );
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[{"kind":"payMana","manaCost":"{1}"},{"kind":"tap","object":{"kind":"self"}},{"kind":"payLife","amount":{"kind":"integer","value":3}},{"kind":"putCountersOnSource","counter":"+1/+1","count":{"kind":"integer","value":1}}],"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]});
    let mut source = card_instance("player-0", &definition("cost-source", "Creature"), 304);
    source.definition.rules = vec![rule];
    source.summoning_sick = false;
    engine.state.players[0].battlefield.push(source);
    engine.state.players[0].mana_pool = vec![FloatingMana {
        symbol: "C".to_string(),
        spend_restriction: None,
    }];
    engine.state.step = GameStep::PrecombatMain;
    let before = serde_json::to_value(&engine.state).unwrap();
    let activation = CanonicalActivatedAbilityActivation {
        controller_id: "player-0".to_string(),
        source_definition_id: "cost-source".to_string(),
        ability_rule_index: Some(0),
        targets: BTreeMap::new(),
    };
    let result = engine.activate_canonical_ability(&activation, &mut RefuseChoices);
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("no implicit replacement choice")
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[test]
fn a_plural_counter_instruction_refuses_before_any_recipient_changes() {
    struct RefuseSecond {
        calls: usize,
    }
    impl DecisionProvider for RefuseSecond {
        fn choose(
            &mut self,
            _: &GameState,
            _: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            self.calls += 1;
            if self.calls == 2 {
                Err(EngineError::new("second event refused"))
            } else {
                Ok(0)
            }
        }
    }
    let mut engine = engine();
    install(&mut engine, "addition", vec![counter_addition()]);
    install(&mut engine, "doubling", vec![counter_doubling()]);
    let mut a = recipient();
    a.instance_id = "a".to_string();
    let mut b = a.clone();
    b.instance_id = "b".to_string();
    engine.state.players[0].battlefield.extend([a, b]);
    let before = serde_json::to_value(&engine.state).unwrap();
    let sequence = engine.branching_decision_sequence.get();
    let mut provider = RefuseSecond { calls: 0 };
    assert!(
        engine
            .place_counters_batch(
                vec!["a".to_string(), "b".to_string()],
                "+1/+1",
                1,
                QuantityOrigin::Effect,
                Some("player-1"),
                &mut provider
            )
            .unwrap_err()
            .to_string()
            .contains("second event refused")
    );
    assert_eq!(provider.calls, 2);
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    assert_eq!(engine.branching_decision_sequence.get(), sequence);
}

#[test]
fn copying_an_opponents_permanent_uses_the_real_creator_and_owner() {
    for (kind, creator) in [
        ("createTokenCopyOfPermanent", "player-0"),
        ("createModifiedTokenCopy", "player-1"),
    ] {
        let mut engine = engine();
        install(
            &mut engine,
            "season",
            vec![modifier(
                "tokenCreation",
                "effect",
                json!({"kind":"multiply","left":{"kind":"integer","value":2},"right":{"kind":"eventQuantity"}}),
            )],
        );
        engine.state.players[0].battlefield[0].controller = creator.to_string();
        let source = card_instance("player-0", &definition("executor", "Artifact"), 401);
        engine.state.players[0].battlefield.push(source);
        let copied = card_instance("player-1", &definition("copied", "Creature"), 402);
        let copied_id = copied.instance_id.clone();
        engine.state.players[1].battlefield.push(copied);
        let mut effect = json!({"kind":kind,"object":{"kind":"chosenTarget","id":"copied"},"exileAtNextEndStep":false});
        if creator == "player-1" {
            effect["controller"] = json!({"kind":"chosenTarget","id":"creator"});
        }
        let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[effect]});
        engine.admit_explicit_test_fixture_objects().unwrap();
        engine
            .execute_canonical_rule(
                CanonicalRuleExecution {
                    controller_id: "player-0".to_string(),
                    source_definition_id: "executor".to_string(),
                    rule,
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::from([
                        (
                            "copied".to_string(),
                            TargetRef::Permanent {
                                instance_id: copied_id.clone(),
                            },
                        ),
                        (
                            "creator".to_string(),
                            TargetRef::Player {
                                player_id: creator.to_string(),
                            },
                        ),
                    ]),
                },
                &mut RefuseChoices,
            )
            .unwrap();
        let tokens = engine
            .state
            .players
            .iter()
            .flat_map(|p| &p.battlefield)
            .filter(|p| p.definition.is_token)
            .collect::<Vec<_>>();
        assert_eq!(tokens.len(), 2);
        assert!(tokens.iter().all(|p| p.owner == creator
            && p.controller == creator
            && p.definition.name == "copied"));
        assert_eq!(
            engine
                .state
                .events
                .iter()
                .filter(|e| e.kind == "tokenCreated" && e.player_id.as_deref() == Some(creator))
                .count(),
            2
        );
        let (p, c) = engine.permanent_position(&copied_id).unwrap();
        assert_eq!(
            engine.state.players[p].battlefield[c].controller,
            "player-1"
        );
    }
}

#[test]
fn explicit_counter_actor_binding_and_affected_replacement_controller_are_independent() {
    for explicit in [false, true] {
        let mut engine = engine();
        install(&mut engine, "addition", vec![counter_addition()]);
        install(&mut engine, "doubling", vec![counter_doubling()]);
        for source in &mut engine.state.players[0].battlefield {
            source.controller = "player-1".to_string();
        }
        let executor = card_instance("player-0", &definition("executor", "Artifact"), 403);
        engine.state.players[0].battlefield.push(executor);
        let card = card_instance("player-1", &definition("recipient", "Creature"), 404);
        let id = card.instance_id.clone();
        engine.state.players[1].battlefield.push(card);
        let mut placement = json!({"kind":"putCounters","permanent":{"kind":"chosenTarget","id":"recipient"},"counter":"+1/+1","count":{"kind":"integer","value":1}});
        if explicit {
            placement["placedBy"] = json!({"kind":"boundValue","id":"capturedController"});
        }
        let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"bind","id":"capturedController","value":{"kind":"controllerOf","object":{"kind":"chosenTarget","id":"recipient"}}},placement]});
        let mut provider = PickSource {
            first: "addition".to_string(),
            expected_player: "player-1".to_string(),
            calls: 0,
        };
        engine.admit_explicit_test_fixture_objects().unwrap();
        engine
            .execute_canonical_rule(
                CanonicalRuleExecution {
                    controller_id: "player-0".to_string(),
                    source_definition_id: "executor".to_string(),
                    rule,
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::from([(
                        "recipient".to_string(),
                        TargetRef::Permanent { instance_id: id },
                    )]),
                },
                &mut provider,
            )
            .unwrap();
        let event = engine
            .state
            .events
            .iter()
            .find(|e| e.kind == "countersPlaced")
            .unwrap();
        assert_eq!(
            event.detail["placedByPlayerId"],
            json!(if explicit { "player-1" } else { "player-0" })
        );
        assert_eq!(event.player_id.as_deref(), Some("player-1"));
        assert_eq!(event.detail["count"], json!(4));
        assert_eq!(provider.calls, 1);
    }
}

#[test]
fn missing_explicit_counter_actor_and_out_of_scope_quantity_do_not_default() {
    let mut engine = engine();
    let source = card_instance("player-0", &definition("executor", "Creature"), 405);
    engine.state.players[0].battlefield.push(source);
    engine.admit_explicit_test_fixture_objects().unwrap();
    let before = serde_json::to_value(&engine.state).unwrap();
    let effect = json!({"kind":"putCounters","permanent":{"kind":"self"},"counter":"charge","count":{"kind":"integer","value":1},"placedBy":{"kind":"boundValue","id":"missing"}});
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[effect.clone()]});
    assert!(rule_is_executable(&rule));
    let error = engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "player-0".to_string(),
                source_definition_id: "executor".to_string(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut RefuseChoices,
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("explicit counter placement actor is unresolved")
    );
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    for count in [
        json!({"kind":"eventQuantity"}),
        json!({"kind":"add","left":{"kind":"eventQuantity"},"right":{"kind":"integer","value":1}}),
    ] {
        let mut out_of_scope = effect.clone();
        out_of_scope["count"] = count;
        assert!(!effect_supported(&out_of_scope));
    }
}

#[test]
fn counter_cost_triggers_are_published_above_the_activated_ability() {
    let mut engine = engine();
    install(&mut engine, "addition", vec![counter_addition()]);
    let activation_rule = json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[{"kind":"putCountersOnSource","counter":"+1/+1","count":{"kind":"integer","value":1}}],"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}]});
    let mut source = card_instance("player-0", &definition("cost-source", "Creature"), 406);
    source.definition.rules = vec![
        activation_rule,
        json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"countersPlaced","player":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Creature"},"object":{"kind":"self"},"counter":"+1/+1"},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":5}}]}),
    ];
    assert!(source.definition.rules.iter().all(rule_is_executable));
    engine.state.players[0].battlefield.push(source);
    engine.state.step = GameStep::PrecombatMain;
    let activation = CanonicalActivatedAbilityActivation {
        controller_id: "player-0".to_string(),
        source_definition_id: "cost-source".to_string(),
        ability_rule_index: Some(0),
        targets: BTreeMap::new(),
    };
    engine
        .activate_canonical_ability(&activation, &mut RefuseChoices)
        .unwrap();
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .last()
            .unwrap()
            .counters
            .get("+1/+1"),
        Some(&2)
    );
    let placements = engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "countersPlaced")
        .collect::<Vec<_>>();
    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].detail["count"], json!(2));
    assert_eq!(engine.state.stack.len(), 2);
    assert_eq!(
        engine.state.stack[0].ability_kind.as_deref(),
        Some("activatedAbility")
    );
    assert_eq!(
        engine.state.stack[1].ability_kind.as_deref(),
        Some("triggeredAbility")
    );
    engine.resolve_top_stack(&mut RefuseChoices).unwrap();
    assert_eq!(engine.state.players[0].life, 25);
    engine.resolve_top_stack(&mut RefuseChoices).unwrap();
    assert_eq!(engine.state.players[0].life, 26);
}

fn execute_token_batch(
    engine: &mut GameEngine,
    amount: i32,
    provider: &mut impl DecisionProvider,
) -> Result<(), EngineError> {
    if !engine.state.players[0]
        .battlefield
        .iter()
        .any(|c| c.definition.id == "executor")
    {
        engine.state.players[0].battlefield.push(card_instance(
            "player-0",
            &definition("executor", "Artifact"),
            501,
        ));
    }
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"createTokens","controller":{"kind":"abilityController"},"quantity":{"kind":"integer","value":amount},"token":{"name":"Batch Witness","types":["Creature"],"power":1,"toughness":1}}]});
    engine.execute_canonical_rule(
        CanonicalRuleExecution {
            controller_id: "player-0".to_string(),
            source_definition_id: "executor".to_string(),
            rule,
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
        },
        provider,
    )
}

#[test]
fn a_hundred_token_batch_uses_one_global_quantity_checkpoint() {
    let mut engine = engine();
    install(
        &mut engine,
        "season",
        vec![modifier(
            "tokenCreation",
            "effect",
            json!({"kind":"multiply","left":{"kind":"integer","value":2},"right":{"kind":"eventQuantity"}}),
        )],
    );
    execute_token_batch(&mut engine, 100, &mut RefuseChoices).unwrap();
    assert_eq!(engine.quantity_snapshot_count(), 1);
    assert!(!engine.quantity_transaction_in_progress());
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|p| p.definition.is_token)
            .count(),
        200
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|e| e.kind == "tokenCreated")
            .count(),
        200
    );
}

#[test]
fn no_active_quantity_program_allocates_no_quantity_checkpoint() {
    let mut engine = engine();
    execute_token_batch(&mut engine, 100, &mut RefuseChoices).unwrap();
    assert_eq!(engine.quantity_snapshot_count(), 0);
    assert!(!engine.quantity_transaction_in_progress());
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|p| p.definition.is_token)
            .count(),
        100
    );
}

#[test]
fn late_entry_refusal_rolls_back_the_entire_batch_and_releases_the_guard_for_retry() {
    struct RefuseLate {
        calls: usize,
    }
    impl DecisionProvider for RefuseLate {
        fn choose(
            &mut self,
            _: &GameState,
            _: &EngineDecisionRequest,
        ) -> Result<usize, EngineError> {
            self.calls += 1;
            if self.calls == 2 {
                Err(EngineError::new("late entry choice refused"))
            } else {
                Ok(0)
            }
        }
    }
    let mut engine = engine();
    install(
        &mut engine,
        "season",
        vec![
            modifier(
                "tokenCreation",
                "effect",
                json!({"kind":"multiply","left":{"kind":"integer","value":2},"right":{"kind":"eventQuantity"}}),
            ),
            counter_doubling(),
        ],
    );
    install(
        &mut engine,
        "riot",
        vec![
            json!({"kind":"grantKeyword","objects":{"kind":"permanents","controller":{"kind":"controllerOf","object":{"kind":"self"}},"where":{"kind":"cardTypeContains","value":"Creature"}},"keyword":"riot"}),
        ],
    );
    engine.state.players[0].battlefield.push(card_instance(
        "player-0",
        &definition("executor", "Artifact"),
        501,
    ));
    let before = serde_json::to_value(&engine.state).unwrap();
    let branches = engine.branching_decision_sequence.get();
    let mut refusing = RefuseLate { calls: 0 };
    assert!(
        execute_token_batch(&mut engine, 3, &mut refusing)
            .unwrap_err()
            .to_string()
            .contains("late entry choice refused")
    );
    assert_eq!(refusing.calls, 2);
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    assert_eq!(engine.branching_decision_sequence.get(), branches);
    assert_eq!(engine.quantity_snapshot_count(), 1);
    assert!(!engine.quantity_transaction_in_progress());
    let mut retry = PickSource {
        first: "unused".to_string(),
        expected_player: "player-0".to_string(),
        calls: 0,
    };
    execute_token_batch(&mut engine, 3, &mut retry).unwrap();
    assert_eq!(retry.calls, 6);
    assert_eq!(engine.quantity_snapshot_count(), 2);
    assert!(!engine.quantity_transaction_in_progress());
    let tokens = engine.state.players[0]
        .battlefield
        .iter()
        .filter(|p| p.definition.is_token)
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 6);
    assert!(tokens.iter().all(|p| p.counters.get("+1/+1") == Some(&2)));
}

#[test]
fn a_handled_nested_failure_cannot_commit_a_partial_quantity_transaction() {
    let mut engine = engine();
    install(&mut engine, "addition", vec![counter_addition()]);
    let before = serde_json::to_value(&engine.state).unwrap();
    let result = engine.with_quantity_transaction(|engine| {
        let failed = engine.with_quantity_transaction(|engine| {
            engine.state.players[0].life -= 1;
            Err::<(), _>(EngineError::new("nested failure"))
        });
        assert!(failed.is_err());
        engine.state.players[0].life += 2;
        Ok(())
    });
    assert!(result.unwrap_err().to_string().contains("nested failure"));
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
    assert_eq!(engine.quantity_snapshot_count(), 1);
    assert!(!engine.quantity_transaction_in_progress());
}

#[test]
fn modified_copy_explicit_creator_owns_controls_contains_and_publishes_each_token() {
    let mut engine = engine();
    install(
        &mut engine,
        "season",
        vec![
            modifier(
                "tokenCreation",
                "effect",
                json!({"kind":"multiply","left":{"kind":"integer","value":2},"right":{"kind":"eventQuantity"}}),
            ),
            counter_doubling(),
        ],
    );
    engine.state.players[0].battlefield[0].controller = "player-1".to_string();
    let executor = card_instance("player-0", &definition("executor", "Artifact"), 601);
    engine.state.players[0].battlefield.push(executor);
    let copied = card_instance("player-0", &definition("copied", "Creature"), 602);
    let copied_id = copied.instance_id.clone();
    engine.state.players[0].battlefield.push(copied);
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[{"kind":"createModifiedTokenCopy","object":{"kind":"chosenTarget","id":"copied"},"controller":{"kind":"chosenTarget","id":"creator"},"counters":[{"counter":"charge","count":{"kind":"integer","value":1}}]}]});
    engine.admit_explicit_test_fixture_objects().unwrap();
    engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "player-0".to_string(),
                source_definition_id: "executor".to_string(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::from([
                    (
                        "copied".to_string(),
                        TargetRef::Permanent {
                            instance_id: copied_id,
                        },
                    ),
                    (
                        "creator".to_string(),
                        TargetRef::Player {
                            player_id: "player-1".to_string(),
                        },
                    ),
                ]),
            },
            &mut RefuseChoices,
        )
        .unwrap();
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|p| !p.definition.is_token)
    );
    let tokens = engine.state.players[1]
        .battlefield
        .iter()
        .filter(|p| p.definition.is_token)
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 2);
    assert!(tokens.iter().all(|p| p.owner == "player-1"
        && p.controller == "player-1"
        && p.counters.get("charge") == Some(&2)));
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|e| e.kind == "tokenCreated" && e.player_id.as_deref() == Some("player-1"))
            .count(),
        2
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|e| e.kind == "tokenCreated" && e.player_id.as_deref() == Some("player-0"))
            .count(),
        0
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|e| e.kind == "countersPlaced"
                && e.player_id.as_deref() == Some("player-1")
                && e.detail["placedByPlayerId"] == json!("player-1")
                && e.detail["count"] == json!(2))
            .count(),
        2
    );
}
