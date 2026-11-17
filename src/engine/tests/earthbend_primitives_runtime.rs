use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn triggering_return() -> Value {
    json!({
        "kind": "moveTriggeringCard",
        "from": { "kind": "triggeringDestination" },
        "to": "battlefield",
        "controller": { "kind": "abilityController" },
        "tapped": true,
    })
}

fn lowered_earthbend_rule(quantity: i64) -> Value {
    let land = json!({ "kind": "chosenTarget", "id": "earthbendLand" });
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [
            { "kind": "payMana", "manaCost": "{2}{G}" },
            { "kind": "tap", "object": { "kind": "self" } },
        ],
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "earthbendLand",
                "kind": "chooseTargets",
                "candidates": {
                    "kind": "permanents",
                    "controller": controller(),
                    "where": { "kind": "cardTypeContains", "value": "Land" },
                },
                "minimum": 1,
                "maximum": 1,
            }],
        },
        "activationCondition": { "kind": "sorceryTiming" },
        "effects": [
            {
                "kind": "becomeCreature",
                "object": land.clone(),
                "addTypes": ["Creature"],
                "addSubtypes": [],
                "basePower": { "kind": "integer", "value": 0 },
                "baseToughness": { "kind": "integer", "value": 0 },
                "retainExistingTypes": true,
                "duration": { "kind": "permanent" },
            },
            {
                "kind": "grantKeyword",
                "object": land.clone(),
                "keyword": "haste",
                "duration": { "kind": "permanent" },
            },
            {
                "kind": "putCounters",
                "permanent": land.clone(),
                "counter": "+1/+1",
                "count": { "kind": "integer", "value": quantity },
            },
            {
                "kind": "installDelayedZoneChangeTrigger",
                "trackedObject": land,
                "controller": { "kind": "abilityController" },
                "destinations": ["graveyard", "exile"],
                "effects": [triggering_return()],
            },
            {
                "kind": "emitAbilitySignal",
                "abilityId": "earthbend",
                "player": { "kind": "abilityController" },
            },
        ],
    })
}

fn setup_lowered_earthbend() -> (GameEngine, Value, StackObject) {
    let rule = lowered_earthbend_rule(2);
    assert!(rule_is_executable(&rule));
    assert!(!rule.to_string().contains("\"kind\":\"earthbend\""));
    let mut engine = test_engine(2);
    let source = test_instance(
        "earthbend-source",
        test_definition("earthbend-source", "Land"),
        "player-0",
    );
    engine.state.players[0].battlefield.push(source.clone());
    let stack = StackObject {
        id: "stack:lowered-earthbend".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "earthbendLand".to_string(),
            TargetRef::Permanent {
                instance_id: "earthbend-source".to_string(),
            },
        )]),
    };
    (engine, rule, stack)
}

fn resolve_earthbend(engine: &mut GameEngine, rule: &Value, stack: &StackObject) {
    engine
        .execute_effects(
            &rule["effects"],
            stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("lowered Earthbend primitives resolve");
}

#[test]
fn death_creates_a_real_trigger_then_returns_the_new_card_tapped_and_reset() {
    let (mut engine, rule, stack) = setup_lowered_earthbend();
    resolve_earthbend(&mut engine, &rule, &stack);

    let permanent = &engine.state.players[0].battlefield[0];
    assert!(permanent.definition.type_line.contains("Creature"));
    assert_eq!(permanent.definition.power.as_deref(), Some("0"));
    assert_eq!(permanent.definition.toughness.as_deref(), Some("0"));
    assert_eq!(permanent.counters.get("+1/+1"), Some(&2));
    assert!(has_keyword(&permanent.definition, "haste"));
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "abilityPerformed" && event.detail["abilityId"] == "earthbend"
    }));

    engine
        .move_permanent_to_graveyard("earthbend-source", "destroyed")
        .expect("animated land dies");
    assert!(engine.permanent_position("earthbend-source").is_none());
    assert_eq!(engine.state.stack.len(), 1, "return waits on the stack");
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "earthbend-source")
    );

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("delayed return resolves");
    let (player_index, card_index) = engine
        .permanent_position("earthbend-source")
        .expect("land returned");
    let returned = &engine.state.players[player_index].battlefield[card_index];
    assert_eq!(returned.controller, "player-0");
    assert!(returned.tapped);
    assert_eq!(returned.definition.type_line, "Land");
    assert!(returned.counters.is_empty());
    assert!(!has_keyword(&returned.definition, "haste"));
}

#[test]
fn exile_uses_the_same_delayed_trigger_and_captured_controller() {
    let (mut engine, rule, stack) = setup_lowered_earthbend();
    resolve_earthbend(&mut engine, &rule, &stack);
    let (player_index, card_index) = engine
        .permanent_position("earthbend-source")
        .expect("animated land exists");
    let mut stolen = engine.state.players[player_index]
        .battlefield
        .remove(card_index);
    stolen.controller = "player-1".to_string();
    engine.state.players[1].battlefield.push(stolen);

    engine
        .exile_permanent("earthbend-source")
        .expect("opponent-controlled land is exiled");
    assert_eq!(engine.state.stack.len(), 1);
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("captured-controller return resolves");

    let (returned_player, returned_index) = engine
        .permanent_position("earthbend-source")
        .expect("exiled land returns");
    assert_eq!(returned_player, 0);
    assert_eq!(
        engine.state.players[returned_player].battlefield[returned_index].controller,
        "player-0"
    );
}

#[test]
fn returned_object_is_present_when_cards_left_graveyard_triggers_are_collected() {
    let (mut engine, rule, stack) = setup_lowered_earthbend();
    let self_trigger = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "cardsLeftGraveyard",
            "player": controller(),
            "where": { "kind": "cardTypeContains", "value": "Land" },
        },
        "effects": [{
            "kind": "gainLife",
            "player": controller(),
            "amount": { "kind": "integer", "value": 1 },
        }],
    });
    let source = &mut engine.state.players[0].battlefield[0];
    let printed = source
        .printed_definition
        .as_mut()
        .expect("test permanent has a printed definition");
    printed.rules.push(self_trigger);
    source.definition = printed.clone();
    resolve_earthbend(&mut engine, &rule, &stack);
    engine
        .move_permanent_to_graveyard("earthbend-source", "destroyed")
        .expect("animated land dies");

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("delayed return resolves first");
    assert!(engine.permanent_position("earthbend-source").is_some());
    let graveyard_leave_triggers = engine
        .state
        .stack
        .iter()
        .filter(|object| {
            object.ability_rule.as_ref().is_some_and(|rule| {
                value_kind(&rule["event"]) == Some("cardsLeftGraveyard")
                    && object.card.instance_id == "earthbend-source"
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(graveyard_leave_triggers.len(), 1);
    let graveyard_leave_stack_id = &graveyard_leave_triggers[0].id;
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| {
                event.kind == "triggeredAbilityPutOnStack"
                    && event.detail["stackId"].as_str() == Some(graveyard_leave_stack_id.as_str())
                    && event.card_instance_id.as_deref() == Some("earthbend-source")
                    && event.detail["additionalTrigger"].as_bool() == Some(false)
            })
            .count(),
        1,
        "the single graveyard-leave trigger is recorded exactly once",
    );
    while engine.state.players[0].life == 20 && !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("pending triggers resolve");
    }
    assert_eq!(engine.state.players[0].life, 21);
}

#[test]
fn return_is_a_no_op_if_the_triggering_zone_object_moved_before_resolution() {
    let (mut engine, rule, stack) = setup_lowered_earthbend();
    resolve_earthbend(&mut engine, &rule, &stack);
    engine
        .move_permanent_to_graveyard("earthbend-source", "destroyed")
        .expect("animated land dies");
    let mut moved =
        GameEngine::take_card_from_player_zones(&mut engine.state.players[0], "earthbend-source")
            .expect("triggering card leaves its graveyard");
    moved.flags.clear();
    engine.state.players[0].hand.push(moved);

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("stale delayed trigger resolves harmlessly");
    assert!(engine.permanent_position("earthbend-source").is_none());
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "earthbend-source")
    );
}

#[test]
fn repeated_zone_occurrence_with_the_same_instance_id_is_not_the_tracked_object() {
    let (mut engine, rule, stack) = setup_lowered_earthbend();
    resolve_earthbend(&mut engine, &rule, &stack);
    engine
        .move_permanent_to_graveyard("earthbend-source", "destroyed")
        .expect("animated land dies");
    let moved =
        GameEngine::take_card_from_player_zones(&mut engine.state.players[0], "earthbend-source")
            .expect("old occurrence leaves graveyard");
    engine.state.players[0].graveyard.push(moved);

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("trigger does not mistake the new occurrence for the old one");
    assert!(engine.permanent_position("earthbend-source").is_none());
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "earthbend-source")
    );
}

#[test]
fn prohibited_graveyard_return_leaves_the_card_and_emits_no_left_graveyard_trigger() {
    let (mut engine, rule, stack) = setup_lowered_earthbend();
    resolve_earthbend(&mut engine, &rule, &stack);

    let mut prohibition = test_definition("entry-prohibition", "Artifact");
    prohibition.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "prohibitEnterBattlefieldFromZones",
            "zones": ["graveyard"],
            "where": { "kind": "cardTypeContains", "value": "Land" },
        }],
    })];
    engine.state.players[1].battlefield.push(test_instance(
        "entry-prohibition",
        prohibition,
        "player-1",
    ));
    let mut watcher = test_definition("graveyard-watcher", "Creature");
    watcher.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "cardsLeftGraveyard",
            "player": controller(),
            "where": { "kind": "cardTypeContains", "value": "Land" },
        },
        "effects": [{
            "kind": "gainLife",
            "player": controller(),
            "amount": { "kind": "integer", "value": 1 },
        }],
    })];
    engine.state.players[0].battlefield.push(test_instance(
        "graveyard-watcher",
        watcher,
        "player-0",
    ));

    engine
        .move_permanent_to_graveyard("earthbend-source", "destroyed")
        .expect("animated land dies");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("prohibited return resolves");

    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "earthbend-source")
    );
    assert!(engine.state.stack.is_empty());
    assert_eq!(engine.state.players[0].life, 20);
}

#[test]
fn delayed_zone_primitives_reject_unsupported_destinations_and_static_sources() {
    assert!(!effect_supported(&json!({
        "kind": "installDelayedZoneChangeTrigger",
        "trackedObject": { "kind": "chosenTarget", "id": "land" },
        "controller": controller(),
        "destinations": ["hand"],
        "effects": [triggering_return()],
    })));
    assert!(!effect_supported(&json!({
        "kind": "moveTriggeringCard",
        "from": { "kind": "graveyard" },
        "to": "battlefield",
        "controller": { "kind": "abilityController" },
        "tapped": true,
    })));
}

#[test]
fn green_mana_ability_remains_available_while_earthbend_obeys_sorcery_timing() {
    let mana_rule = json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [{
            "kind": "addMana",
            "player": controller(),
            "mana": "{G}",
        }],
    });
    let earthbend_rule = lowered_earthbend_rule(2);
    let mut definition = test_definition("test-earthbend-land", "Land");
    definition.rules = vec![mana_rule, earthbend_rule];
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].battlefield.push(test_instance(
        "test-earthbend-land",
        definition,
        "player-0",
    ));
    engine.state.players[0].mana_pool = vec![
        FloatingMana {
            symbol: "C".to_string(),
            spend_restriction: None,
        },
        FloatingMana {
            symbol: "C".to_string(),
            spend_restriction: None,
        },
        FloatingMana {
            symbol: "G".to_string(),
            spend_restriction: None,
        },
    ];

    let has_rule = |engine: &GameEngine, rule_index: u64| {
        engine.legal_priority_actions(0).into_iter().any(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("test-earthbend-land")
                && action.decisions.get("abilityRuleIndex") == Some(&Value::from(rule_index))
        })
    };
    assert!(has_rule(&engine, 0), "tap-for-green mana is legal");
    assert!(
        has_rule(&engine, 1),
        "Earthbend is legal in the active main phase"
    );

    engine.state.step = GameStep::DeclareAttackers;
    assert!(has_rule(&engine, 0), "mana ability ignores sorcery timing");
    assert!(
        !has_rule(&engine, 1),
        "Earthbend is not legal during combat"
    );

    engine.state.step = GameStep::PrecombatMain;
    engine.state.stack.push(StackObject {
        id: "stack:block-sorcery-timing".to_string(),
        controller: "player-1".to_string(),
        card: test_instance(
            "blocking-spell",
            test_definition("blocking-spell", "Instant"),
            "player-1",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [],
        })),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    assert!(has_rule(&engine, 0));
    assert!(!has_rule(&engine, 1), "Earthbend requires an empty stack");
}
