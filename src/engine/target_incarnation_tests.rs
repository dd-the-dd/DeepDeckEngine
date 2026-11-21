use super::super::*;
use serde_json::json;

struct FirstChoice;
impl DecisionProvider for FirstChoice {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(0)
    }
}

fn definition(id: &str, type_line: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        mana_cost: "{0}".to_string(),
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        rules: Vec::new(),
    }
}

fn position() -> (GameEngine, StackObject, String) {
    let filler = definition("filler", "Basic Land - Plains");
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("player-{index}"),
                    name: format!("Player {index}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 7,
            starting_player: 0,
        },
        1714,
    )
    .unwrap();
    let source = card_instance("player-0", &definition("source", "Artifact"), 0);
    let mut target = card_instance("player-0", &definition("recipient", "Creature - Scout"), 1);
    GameEngine::assign_tracked_zone_identity(&mut target, "battlefield:designated");
    let source_id = source.instance_id.clone();
    let target_id = target.instance_id.clone();
    engine.state.players[0]
        .battlefield
        .extend([source.clone(), target]);
    let rule = json!({
        "kind":"activatedAbility", "source":{"kind":"self"},
        "costs":[{"kind":"payMana","manaCost":"{0}"}],
        "declaration":{"kind":"castingDeclaration","decisions":[{
            "kind":"chooseTargets","id":"recipient","minimum":1,"maximum":1,
            "candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}},
        }]},
        "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":{"kind":"integer","value":1}}],
    });
    assert!(
        rule_is_executable(&rule),
        "complete target identity test activation"
    );
    let declarations = engine
        .declarations_for_activated_rule(&rule, "player-0", &source_id)
        .expect("valid shared declaration");
    let declaration = declarations.into_iter().next().expect("one creature");
    let stack = StackObject {
        id: "stack:target-incarnation".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule),
        decisions: declaration.decisions,
        targets: declaration.targets,
    };
    (engine, stack, target_id)
}

fn return_same_physical_card(engine: &mut GameEngine, target_id: &str) {
    engine.exile_permanent(target_id).unwrap();
    let owner = engine
        .state
        .players
        .iter()
        .position(|player| {
            player
                .exile
                .iter()
                .any(|card| card.instance_id == target_id)
        })
        .unwrap();
    let index = engine.state.players[owner]
        .exile
        .iter()
        .position(|card| card.instance_id == target_id)
        .unwrap();
    let mut returned = engine.state.players[owner].exile.remove(index);
    GameEngine::assign_tracked_zone_identity(&mut returned, "battlefield:returned");
    engine.state.players[owner].battlefield.push(returned);
}

#[test]
fn designated_incarnation_is_not_recaptured_at_resolution_after_blink() {
    let (mut engine, stack, target_id) = position();
    let target = &stack.targets["recipient"];
    assert!(engine.target_is_legal_for_stack_object("recipient", target, &stack));
    return_same_physical_card(&mut engine, &target_id);
    assert!(
        engine.target_exists(target),
        "same physical iid is present again"
    );
    assert!(!engine.target_is_legal_for_stack_object("recipient", target, &stack));
}

#[test]
fn stable_incarnation_revalidates_type_and_control_without_becoming_new_object() {
    let (mut engine, stack, target_id) = position();
    let (player, index) = engine.permanent_position(&target_id).unwrap();
    let mut target = engine.state.players[player].battlefield.remove(index);
    target.controller = "player-1".to_string();
    engine.state.players[1].battlefield.push(target);
    assert!(engine.target_is_legal_for_stack_object(
        "recipient",
        &stack.targets["recipient"],
        &stack
    ));
    engine.state.players[1].battlefield[0].definition.type_line = "Artifact".to_string();
    assert!(engine.target_incarnation_is_current(&stack, "recipient", &stack.targets["recipient"]));
    assert!(!engine.target_is_legal_for_stack_object(
        "recipient",
        &stack.targets["recipient"],
        &stack
    ));
}

#[test]
fn copy_keeps_old_identity_then_explicit_retarget_accepts_returned_incarnation() {
    let (mut engine, stack, target_id) = position();
    engine.state.stack.push(stack.clone());
    let copy_id = engine.copy_stack_object(&stack.id).unwrap();
    let copy = engine
        .state
        .stack
        .iter()
        .find(|item| item.id == copy_id)
        .unwrap();
    assert_eq!(
        copy.decisions, stack.decisions,
        "unchanged copied targets retain capture"
    );
    return_same_physical_card(&mut engine, &target_id);
    for item in &engine.state.stack {
        assert!(!engine.target_is_legal_for_stack_object(
            "recipient",
            &item.targets["recipient"],
            item
        ));
    }
    engine
        .choose_stack_target(&copy_id, "recipient", "player-0", &mut FirstChoice)
        .unwrap();
    let copied = engine
        .state
        .stack
        .iter()
        .find(|item| item.id == copy_id)
        .unwrap();
    assert!(engine.target_is_legal_for_stack_object(
        "recipient",
        &copied.targets["recipient"],
        copied
    ));
    let original = engine
        .state
        .stack
        .iter()
        .find(|item| item.id == stack.id)
        .unwrap();
    assert!(!engine.target_is_legal_for_stack_object(
        "recipient",
        &original.targets["recipient"],
        original
    ));
}

#[test]
fn nontargeted_chosen_objects_are_not_recorded_as_declared_targets() {
    let (engine, _stack, target_id) = position();
    let target = TargetRef::Permanent {
        instance_id: target_id,
    };
    let mut declaration = CastingDeclaration {
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([("costObject".to_string(), target)]),
        target_order: Vec::new(),
    };
    engine
        .capture_declared_target_incarnations(&mut declaration)
        .expect("test target must have an admitted incarnation");
    assert!(declaration.decisions.is_empty());
}

fn admitted_card_target_position(activated: bool) -> (GameEngine, StackObject, String) {
    let filler = definition("card-filler", "Basic Land - Plains");
    let mut e = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("player-{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        181401,
    )
    .unwrap();
    let mut source = definition(
        "card-target-source",
        if activated { "Artifact" } else { "Instant" },
    );
    let mut rule = json!({"kind":"spellAbility","source":{"kind":"self"},"declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"card","minimum":1,"maximum":1,"candidates":{"kind":"cards","zone":{"kind":"graveyard","player":{"kind":"controllerOf","object":{"kind":"self"}}},"where":{"kind":"cardTypeContains","value":"Creature"}}}]},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]});
    if activated {
        rule["kind"] = json!("activatedAbility");
        rule["costs"] = json!([{"kind":"payMana","manaCost":"{0}"}]);
    }
    assert!(rule_is_executable(&rule));
    source.rules = vec![rule.clone()];
    let source = card_instance("player-0", &source, 84);
    if activated {
        e.state.players[0].battlefield.push(source.clone());
    } else {
        e.state.players[0].hand.push(source.clone());
    }
    let target = card_instance(
        "player-0",
        &definition("card-recipient", "Creature - Scout"),
        85,
    );
    let target_id = target.instance_id.clone();
    e.state.players[0].graveyard.push(target);
    e.state.players[1].graveyard.push(card_instance(
        "player-1",
        &definition("opponent-card-recipient", "Creature - Scout"),
        86,
    ));
    e.admit_explicit_test_fixture_objects()
        .expect("construction admission, before any target reference");
    let source = if activated {
        e.state.players[0].battlefield.last().unwrap().clone()
    } else {
        e.state.players[0].hand.last().unwrap().clone()
    };
    let mut stack = StackObject {
        id: "stack:admitted-card-target".into(),
        controller: "player-0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some(
            if activated {
                "activatedAbility"
            } else {
                "spellAbility"
            }
            .into(),
        ),
        ability_rule: Some(rule),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "card".into(),
            TargetRef::Card {
                instance_id: target_id.clone(),
            },
        )]),
    };
    e.capture_target_incarnation(&mut stack.decisions, "card", &stack.targets["card"])
        .unwrap();
    (e, stack, target_id)
}

#[test]
fn admitted_card_designation_preserves_readonly_identity_and_shared_zone_partition() {
    let (mut e, mut stack, id) = admitted_card_target_position(true);
    let clock = e.state.battlefield_incarnation_sequence;
    assert!(e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));
    assert!(e.validate_resolution_target_authentication(&stack).is_ok());
    e.state.players[0].graveyard.reverse();
    assert!(e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));
    assert_eq!(
        e.state.battlefield_incarnation_sequence, clock,
        "capture and reads never allocate"
    );
    // Explicit construction of an admitted exile object, not a runtime zone
    // writer proof. Storage-player changes within shared exile are not moves.
    let mut exiled = e.state.players[0].graveyard.pop().unwrap();
    assert_eq!(exiled.instance_id, id);
    e.assign_new_object_incarnation(&mut exiled).unwrap();
    e.state.players[0].exile.push(exiled);
    e.capture_target_incarnation(&mut stack.decisions, "card", &stack.targets["card"])
        .unwrap();
    let captured = stack.decisions.clone();
    let card = e.state.players[0].exile.pop().unwrap();
    e.state.players[1].exile.push(card);
    assert!(e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));
    assert_eq!(stack.decisions, captured);
    // Same physical ID with a genuinely new admitted epoch must not inherit
    // the old designation. Native writer integration remains a separate proof.
    let mut card = e.state.players[1].exile.pop().unwrap();
    e.assign_new_object_incarnation(&mut card).unwrap();
    e.state.players[1].exile.push(card);
    assert!(e.validate_resolution_target_authentication(&stack).is_ok());
    assert!(!e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));
}

#[test]
fn card_authentication_errors_are_not_target_departure_or_resolution_repair() {
    let (mut e, stack, id) = admitted_card_target_position(true);
    let mut missing = stack.clone();
    missing.decisions.remove(super::CAPTURED_CARD_TARGETS);
    assert!(
        e.validate_resolution_target_authentication(&missing)
            .is_err()
    );
    e.state.stack.push(missing);
    let before = serde_json::to_value(&e.state).unwrap();
    assert!(
        e.resolve_top_stack(&mut FirstChoice)
            .unwrap_err()
            .message
            .contains("target authentication context")
    );
    assert_eq!(
        serde_json::to_value(&e.state).unwrap(),
        before,
        "read-only rejection before pop; no lazy designation"
    );
    e.state.stack.clear();
    let mut malformed = stack.clone();
    malformed
        .decisions
        .get_mut(super::CAPTURED_CARD_TARGETS)
        .unwrap()["card"]["identity"]["zoneIdentity"] = json!("");
    assert!(
        e.validate_resolution_target_authentication(&malformed)
            .is_err()
    );
    let duplicate = e.state.players[0].graveyard[0].clone();
    e.state.players[1].graveyard.push(duplicate);
    let mut decisions = BTreeMap::from([("unchanged".into(), json!(true))]);
    let prior = decisions.clone();
    assert!(
        e.capture_target_incarnation(&mut decisions, "card", &TargetRef::Card { instance_id: id })
            .is_err()
    );
    assert_eq!(
        decisions, prior,
        "ambiguity is an error, not first-carrier selection"
    );
}

#[test]
fn fresh_public_spell_and_activation_capture_card_designations_before_stack_commit() {
    for activated in [false, true] {
        let (mut e, _manual, _) = admitted_card_target_position(activated);
        if activated {
            let activation = serde_json::from_value(
                json!({"controllerId":"player-0","sourceDefinitionId":"card-target-source",
                    "targets":{"card":{"kind":"cardDefinition","definitionId":"card-recipient"}}}),
            )
            .unwrap();
            e.activate_canonical_ability(&activation, &mut FirstChoice)
                .unwrap();
        } else {
            let request = serde_json::from_value(
                json!({"controllerId":"player-0","sourceDefinitionId":"card-target-source"}),
            )
            .unwrap();
            e.cast_canonical_spell(&request, &mut FirstChoice).unwrap();
        }
        let committed = e
            .state
            .stack
            .last()
            .expect("fresh actual public stack commitment");
        GameEngine::captured_card_target_binding(committed, "card")
            .expect("proof produced at declaration, not resolution");
        assert!(
            e.validate_resolution_target_authentication(committed)
                .is_ok()
        );
        assert!(e.target_incarnation_is_current(committed, "card", &committed.targets["card"]));
        e.resolve_top_stack(&mut FirstChoice).unwrap();
        assert_eq!(e.state.players[0].life, 21);
    }
}

fn execute_identity_movement(
    engine: &mut GameEngine,
    source: &StackObject,
    ids: Vec<String>,
    destination: Value,
    random: bool,
) -> Result<(), EngineError> {
    let mut effect = json!({"kind":"moveCards","cards":{"kind":"boundObjects","binding":"moving"},"to":destination});
    if random {
        effect["order"] = json!({"kind":"random"});
    }
    let mut bindings = BTreeMap::from([("moving".to_string(), RuntimeBinding::Objects(ids))]);
    engine.execute_effects(
        &json!([effect]),
        source,
        &mut bindings,
        &mut BTreeMap::new(),
        &mut FirstChoice,
    )
}

#[test]
fn actual_card_moves_leave_return_and_reexile_invalidate_original_designations() {
    let (mut e, mut stack, id) = admitted_card_target_position(true);
    let original = stack.decisions.clone();
    let original_clock = e.state.battlefield_incarnation_sequence;
    execute_identity_movement(
        &mut e,
        &stack,
        vec![id.clone()],
        json!({"kind":"hand","player":{"kind":"abilityController"}}),
        false,
    )
    .unwrap();
    assert!(e.validate_resolution_target_authentication(&stack).is_ok());
    assert!(!e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));
    execute_identity_movement(
        &mut e,
        &stack,
        vec![id.clone()],
        json!({"kind":"graveyard","player":{"kind":"abilityController"}}),
        false,
    )
    .unwrap();
    assert_eq!(e.state.battlefield_incarnation_sequence, original_clock + 2);
    assert_eq!(
        stack.decisions, original,
        "movement never retargets the old designation"
    );
    assert!(!e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));
    e.capture_target_incarnation(&mut stack.decisions, "card", &stack.targets["card"])
        .unwrap();
    execute_identity_movement(
        &mut e,
        &stack,
        vec![id.clone()],
        json!({"kind":"exile","player":{"kind":"abilityController"},"faceDown":false}),
        false,
    )
    .unwrap();
    e.capture_target_incarnation(&mut stack.decisions, "card", &stack.targets["card"])
        .unwrap();
    let before_reexile = e.state.battlefield_incarnation_sequence;
    execute_identity_movement(
        &mut e,
        &stack,
        vec![id],
        json!({"kind":"exile","player":{"kind":"abilityController"},"faceDown":false}),
        false,
    )
    .unwrap();
    assert_eq!(
        e.state.battlefield_incarnation_sequence,
        before_reexile + 1,
        "CR400.8 same exile is a new object"
    );
    assert!(!e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));
}

#[test]
fn random_same_library_reordering_preserves_identity_and_uses_rng_for_both_ends() {
    for position in ["top", "bottom"] {
        let (mut e, stack, _) = admitted_card_target_position(true);
        let clock = e.state.battlefield_incarnation_sequence;
        let library = e.state.players[0].library.clone();
        let ids = library
            .iter()
            .map(|card| card.instance_id.clone())
            .collect::<Vec<_>>();
        let mut expected = library;
        let mut expected_rng = e.rng.clone();
        expected.shuffle(&mut expected_rng);
        if position == "top" {
            expected.reverse();
        }
        execute_identity_movement(
            &mut e,
            &stack,
            ids,
            json!({"kind":"library","player":{"kind":"abilityController"},"position":position}),
            true,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&e.state.players[0].library).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(
            e.state.battlefield_incarnation_sequence, clock,
            "same-library order is not a zone change"
        );
        assert_eq!(
            e.rng.next_u64(),
            expected_rng.next_u64(),
            "same shuffle service and RNG consumption at either end"
        );
    }
}

#[test]
fn exhausted_identity_transition_rolls_back_before_destination_visibility() {
    let (mut e, stack, id) = admitted_card_target_position(true);
    e.state.battlefield_incarnation_sequence = usize::MAX;
    let before = serde_json::to_value(&e.state).unwrap();
    let mut before_rng = e.rng.clone();
    let error = e
        .with_replacement_checkpoint(true, |engine| {
            execute_identity_movement(
                engine,
                &stack,
                vec![id],
                json!({"kind":"hand","player":{"kind":"abilityController"}}),
                false,
            )
        })
        .unwrap_err();
    assert!(error.message.contains("incarnation sequence exhausted"));
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(e.rng.next_u64(), before_rng.next_u64());
}

#[test]
fn prepared_graveyard_replacement_authenticates_value_after_vector_shift_and_rejects_stale_plan() {
    let (mut e, stack, id) = admitted_card_target_position(true);
    let replacement = json!({"kind":"exileSpecificCardInsteadOfGraveyard", "cardInstanceId":id,
        "sourceCardInstanceId":stack.card.instance_id});
    e.state.rule_modifiers.push(replacement.clone());
    let plan = e.prepare_card_graveyard_destination(&id).unwrap();
    // This is not the selected replacement; an index-only plan would consume it.
    e.state
        .rule_modifiers
        .insert(0, json!({"kind":"testUnrelatedMarker"}));
    let moved = e.take_zone_card_for_movement(0, "graveyard", 0).unwrap();
    assert!(
        !e.put_owned_card_into_prepared_graveyard(0, moved, plan)
            .unwrap()
    );
    assert_eq!(
        e.state.rule_modifiers,
        vec![json!({"kind":"testUnrelatedMarker"})]
    );
    assert_eq!(e.state.players[0].exile.last().unwrap().instance_id, id);
    assert!(!e.target_incarnation_is_current(&stack, "card", &stack.targets["card"]));

    let (mut e, _, id) = admitted_card_target_position(true);
    e.state
        .rule_modifiers
        .push(json!({"kind":"exileSpecificCardInsteadOfGraveyard",
        "cardInstanceId":id,"sourceCardInstanceId":"setup-policy"}));
    let plan = e.prepare_card_graveyard_destination(&id).unwrap();
    e.state.rule_modifiers.clear();
    let before = serde_json::to_value(&e.state).unwrap();
    let error = e
        .with_replacement_checkpoint(true, |e| {
            let moved = e.take_zone_card_for_movement(0, "graveyard", 0)?;
            e.put_owned_card_into_prepared_graveyard(0, moved, plan)
        })
        .unwrap_err();
    assert!(error.message.contains("no longer available"));
    assert_eq!(
        serde_json::to_value(&e.state).unwrap(),
        before,
        "stale policy cannot consume another effect, change identity or strand the carrier"
    );
}
