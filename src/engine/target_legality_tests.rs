use super::super::*;
use super::target_ordinal;

fn definition(id: &str, type_line: &str, mana_cost: &str) -> CardDefinition {
    CardDefinition {
        id: id.to_string(),
        name: id.to_string(),
        type_line: type_line.to_string(),
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: mana_cost.to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains", "");
    GameEngine::new(
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
        401,
    )
    .expect("target legality setup is valid")
}

fn stack_object(controller: &str, source: CardInstance, rule: Value) -> StackObject {
    StackObject {
        id: "stack:target-legality".to_string(),
        controller: controller.to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn target_rule(id: &str, candidates: Value) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "permanentEntered", "anyController": true, "where": null },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": id,
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": candidates,
            }],
        },
        "effects": [],
    })
}

#[test]
fn exact_and_numeric_ordinal_ids_do_not_split_modal_namespaces() {
    assert_eq!(
        target_ordinal("targetPermanent", "targetPermanent"),
        Some(None)
    );
    assert_eq!(
        target_ordinal("targetPermanent", "targetPermanent:0"),
        Some(Some(0))
    );
    assert_eq!(
        target_ordinal("targetPermanent", "targetPermanent:12"),
        Some(Some(12))
    );
    assert_eq!(
        target_ordinal("targetPermanent", "targetPermanent:+1"),
        None
    );
    assert_eq!(
        target_ordinal("targetPermanent", "targetPermanent:mode1"),
        None
    );
    assert_eq!(
        target_ordinal("mode1:targetPermanent", "mode1:targetPermanent"),
        Some(None)
    );
    assert_eq!(
        target_ordinal("mode1:targetPermanent", "mode1:targetPermanent:0"),
        Some(Some(0))
    );
    assert_eq!(target_ordinal("mode1", "mode1:targetPermanent"), None);
}

#[test]
fn modal_target_candidates_use_committed_mode_and_x_value() {
    let mut engine = engine();
    let source = card_instance(
        "player-0",
        &definition("modal-source", "Creature - Wizard", "{X}"),
        1,
    );
    let source_copy = source.clone();
    engine.state.players[0].battlefield.push(source);
    let mut low_definition = definition("mv-two", "Artifact Creature", "{2}");
    low_definition.power = Some("2".to_string());
    low_definition.toughness = Some("2".to_string());
    let low = card_instance("player-1", &low_definition, 2);
    let mut high_definition = definition("mv-three", "Artifact Creature", "{3}");
    high_definition.power = Some("3".to_string());
    high_definition.toughness = Some("3".to_string());
    let high = card_instance("player-1", &high_definition, 3);
    let low_target = TargetRef::Permanent {
        instance_id: low.instance_id.clone(),
    };
    engine.state.players[1].battlefield.extend([low, high]);
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "permanentEntered", "anyController": true, "where": null },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [
                { "id": "chosenModes", "kind": "chooseModes", "minimum": 1,
                  "maximum": 1, "options": ["mode1", "mode2"] },
                { "id": "mode1:targetPermanent", "kind": "chooseTargets",
                  "minimum": 1, "maximum": 1,
                  "condition": { "kind": "selectionContains",
                    "selection": { "kind": "decisionResult", "decisionId": "chosenModes" },
                    "value": "mode1" },
                  "candidates": { "kind": "permanents",
                    "controller": { "kind": "opponentsOf",
                      "player": { "kind": "abilityController" } },
                    "where": {
                    "kind": "compare", "operator": "<=",
                    "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                    "right": { "kind": "decisionResult", "decisionId": "xValue" }
                  } } },
                { "id": "mode2:targetPermanent", "kind": "chooseTargets",
                  "minimum": 1, "maximum": 1,
                  "condition": { "kind": "selectionContains",
                    "selection": { "kind": "decisionResult", "decisionId": "chosenModes" },
                    "value": "mode2" },
                  "candidates": { "kind": "permanents",
                    "where": { "kind": "cardTypeContains", "value": "Land" } } }
            ]
        },
        "effects": []
    });
    let mut stack = stack_object("player-0", source_copy, rule);
    stack
        .decisions
        .insert("chosenModes".to_string(), json!(["mode1"]));
    stack.decisions.insert("xValue".to_string(), json!(2));

    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "mode1:targetPermanent"),
        Some(vec![low_target.clone()])
    );
    assert_eq!(
        engine.stack_target_candidates(&stack, "mode1:targetPermanent"),
        vec![low_target.clone()],
        "the retarget path must preserve the modal namespace instead of splitting at ':'"
    );
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "mode2:targetPermanent"),
        Some(Vec::new())
    );
    for property in ["powerOf", "toughnessOf"] {
        let property_rule = target_rule(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "controller": { "kind": "opponentsOf",
                    "player": { "kind": "abilityController" } },
                "where": {
                    "kind": "compare",
                    "operator": "<=",
                    "left": { "kind": property, "object": { "kind": "candidate" } },
                    "right": { "kind": "decisionResult", "decisionId": "xValue" },
                },
            }),
        );
        let mut property_stack = stack_object("player-0", stack.card.clone(), property_rule);
        property_stack
            .decisions
            .insert("xValue".to_string(), json!(2));
        assert_eq!(
            engine.declared_stack_target_candidates(&property_stack, "targetPermanent"),
            Some(vec![low_target.clone()]),
            "{property} must use the printed 2/2 characteristics before modification"
        );
    }
    engine.state.players[1].battlefield[0]
        .counters
        .insert("+1/+1".to_string(), 1);
    for property in ["powerOf", "toughnessOf"] {
        let property_rule = target_rule(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "controller": { "kind": "opponentsOf",
                    "player": { "kind": "abilityController" } },
                "where": {
                    "kind": "compare",
                    "operator": "<=",
                    "left": { "kind": property, "object": { "kind": "candidate" } },
                    "right": { "kind": "decisionResult", "decisionId": "xValue" },
                },
            }),
        );
        let mut property_stack = stack_object("player-0", stack.card.clone(), property_rule);
        property_stack
            .decisions
            .insert("xValue".to_string(), json!(2));
        assert_eq!(
            engine.declared_stack_target_candidates(&property_stack, "targetPermanent"),
            Some(Vec::new()),
            "{property} must observe the +1/+1 counter on the current permanent"
        );
    }
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "mode1:targetPermanent"),
        Some(vec![low_target]),
        "the modal mana-value filter must remain unaffected by a +1/+1 counter"
    );
}

#[test]
fn card_zone_player_is_resolved_from_the_committed_stack_context() {
    let mut engine = engine();
    let source = card_instance(
        "player-0",
        &definition("zone-source", "Creature - Wizard", "{2}{B}"),
        1,
    );
    let controllers_card = card_instance(
        "player-0",
        &definition("controller-card", "Creature - Bear", "{1}"),
        2,
    );
    let chosen_players_card = card_instance(
        "player-1",
        &definition("chosen-player-card", "Creature - Bear", "{1}"),
        3,
    );
    engine.state.players[0]
        .graveyard
        .push(controllers_card.clone());
    engine.state.players[1]
        .graveyard
        .push(chosen_players_card.clone());
    let rule = target_rule(
        "targetCard",
        json!({
            "kind": "cards",
            "zone": { "kind": "graveyard", "player": {
                "kind": "chosenTarget", "id": "targetPlayer"
            } },
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        }),
    );
    let mut stack = stack_object("player-0", source, rule);
    stack.targets.insert(
        "targetPlayer".to_string(),
        TargetRef::Player {
            player_id: "player-1".to_string(),
        },
    );
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "targetCard"),
        Some(vec![TargetRef::Card {
            instance_id: chosen_players_card.instance_id,
        }])
    );
}

#[test]
fn committed_target_does_not_recheck_dynamic_declaration_condition() {
    let mut engine = engine();
    let source = card_instance(
        "player-0",
        &definition("conditional-source", "Creature - Wizard", "{2}"),
        1,
    );
    let source_copy = source.clone();
    let enabling_artifact = card_instance("player-0", &definition("enabler", "Artifact", "{1}"), 2);
    let target = card_instance(
        "player-1",
        &definition("conditional-target", "Creature - Bear", "{2}"),
        3,
    );
    let target_ref = TargetRef::Permanent {
        instance_id: target.instance_id.clone(),
    };
    engine.state.players[0]
        .battlefield
        .extend([source, enabling_artifact]);
    engine.state.players[1].battlefield.push(target);
    let mut rule = target_rule(
        "targetCreature",
        json!({
            "kind": "permanents",
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        }),
    );
    rule["declaration"]["decisions"][0]["condition"] = json!({
        "kind": "controlsPermanent",
        "where": { "kind": "cardTypeContains", "value": "Artifact" },
    });
    let mut stack = stack_object("player-0", source_copy, rule);
    stack
        .targets
        .insert("targetCreature".to_string(), target_ref.clone());
    engine.state.players[0].battlefield.remove(1);

    assert!(
        engine
            .declared_stack_target_candidates(&stack, "targetCreature")
            .is_some_and(|candidates| candidates.contains(&target_ref)),
        "the declaration condition was satisfied when the target was committed"
    );
}

#[test]
fn exclude_source_and_attached_permanent_use_live_source_then_lki() {
    let mut engine = engine();
    let attached = card_instance(
        "player-0",
        &definition("attached", "Creature - Bear", "{2}"),
        2,
    );
    let other = card_instance(
        "player-0",
        &definition("other", "Creature - Bear", "{2}"),
        3,
    );
    let mut source = card_instance(
        "player-0",
        &definition("aura-source", "Enchantment - Aura", "{1}{W}"),
        1,
    );
    source.attached_to = Some(attached.instance_id.clone());
    let source_copy = source.clone();
    engine.state.players[0]
        .battlefield
        .extend([source, attached.clone(), other.clone()]);
    let rule = target_rule(
        "targetCreature",
        json!({
            "kind": "permanents",
            "excludeSource": true,
            "exclude": { "kind": "attachedPermanent", "attachment": { "kind": "self" } },
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        }),
    );
    let stack = stack_object("player-0", source_copy, rule);
    let expected = TargetRef::Permanent {
        instance_id: other.instance_id,
    };
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "targetCreature"),
        Some(vec![expected.clone()])
    );

    engine.state.players[0].battlefield[0].attached_to = None;
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "targetCreature"),
        Some(vec![
            TargetRef::Permanent {
                instance_id: attached.instance_id,
            },
            expected.clone(),
        ]),
        "an empty attachedPermanent reference removes only that exclusion"
    );

    engine.state.players[0].battlefield.remove(0);
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "targetCreature"),
        Some(vec![expected])
    );
}

#[test]
fn resolution_revalidates_each_target_without_reapplying_same_zone_owner() {
    let mut engine = engine();
    let source = card_instance(
        "player-0",
        &definition("grave-trigger", "Creature - Wizard", "{2}{B}"),
        1,
    );
    let first = card_instance(
        "player-1",
        &definition("first-card", "Creature - Bear", "{1}"),
        2,
    );
    let second = card_instance(
        "player-1",
        &definition("second-card", "Creature - Bear", "{1}"),
        3,
    );
    engine.state.players[1]
        .graveyard
        .extend([first.clone(), second.clone()]);
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "permanentEntered", "anyController": true, "where": null },
        "declaration": { "kind": "castingDeclaration", "decisions": [{
            "id": "targetCards", "kind": "chooseTargets", "minimum": 2, "maximum": 2,
            "candidates": { "kind": "cards", "zone": { "kind": "anyGraveyard" } },
            "selectionConstraint": { "kind": "sameZoneOwner", "zone": "graveyard" }
        }] },
        "effects": []
    });
    let mut stack = stack_object("player-0", source, rule);
    let first_target = TargetRef::Card {
        instance_id: first.instance_id.clone(),
    };
    let second_target = TargetRef::Card {
        instance_id: second.instance_id.clone(),
    };
    stack
        .targets
        .insert("targetCards:0".to_string(), first_target.clone());
    stack
        .targets
        .insert("targetCards:1".to_string(), second_target.clone());
    stack.decisions.insert(
        "targetCards".to_string(),
        json!([first_target, second_target.clone()]),
    );

    let moved = engine.state.players[1].graveyard.remove(0);
    engine.state.players[1].hand.push(moved);

    let individual = engine
        .declared_stack_target_candidates(&stack, "targetCards:1")
        .expect("the declaration remains evaluable");
    assert!(individual.contains(&second_target));
    assert!(
        engine
            .retarget_candidates_for_stack_target(&stack, "targetCards:1")
            .is_empty(),
        "a new choice must still satisfy the joint selection constraint"
    );
}

#[test]
fn controller_type_and_zone_changes_remove_only_the_affected_candidate() {
    let mut engine = engine();
    let source = card_instance(
        "player-0",
        &definition("source", "Creature - Wizard", "{2}"),
        1,
    );
    let target = card_instance(
        "player-1",
        &definition("target", "Creature - Bear", "{2}"),
        2,
    );
    let target_ref = TargetRef::Permanent {
        instance_id: target.instance_id.clone(),
    };
    engine.state.players[1].battlefield.push(target);
    let rule = target_rule(
        "targetCreature",
        json!({
            "kind": "permanents",
            "controller": { "kind": "opponentsOf",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } } },
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        }),
    );
    let stack = stack_object("player-0", source, rule);
    assert!(
        engine
            .declared_stack_target_candidates(&stack, "targetCreature")
            .is_some_and(|candidates| candidates.contains(&target_ref))
    );
    engine.state.players[1].battlefield[0].controller = "player-0".to_string();
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "targetCreature"),
        Some(Vec::new())
    );
    engine.state.players[1].battlefield[0].controller = "player-1".to_string();
    engine.state.players[1].battlefield[0].definition.type_line = "Artifact".to_string();
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "targetCreature"),
        Some(Vec::new())
    );
    engine.state.players[1].battlefield.clear();
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "targetCreature"),
        Some(Vec::new())
    );
}

#[test]
fn undeclared_implicit_target_preserves_the_existing_path() {
    let engine = engine();
    let source = card_instance(
        "player-0",
        &definition("aura", "Enchantment - Aura", "{1}{W}"),
        1,
    );
    let stack = stack_object(
        "player-0",
        source,
        json!({
            "kind": "spellAbility", "source": { "kind": "self" }, "effects": []
        }),
    );
    assert_eq!(
        engine.declared_stack_target_candidates(&stack, "enchantTarget"),
        None
    );
}

#[test]
fn empty_selection_receipt_uses_the_same_longest_owner_as_nonempty_members() {
    let candidates = json!({"kind":"permanents","where":null});
    let mut rule = target_rule("cards", candidates.clone());
    rule["declaration"]["decisions"][0]["minimum"] = json!(0);
    let mut nested = rule["declaration"]["decisions"][0].clone();
    nested["id"] = json!("cards:1");
    nested["minimum"] = json!(1);
    rule["declaration"]["decisions"]
        .as_array_mut()
        .unwrap()
        .push(nested);
    // A triggered ability needs an effect program to be executable at all;
    // the declaration shape is the subject of this test.
    rule["effects"] = json!([{
        "kind": "gainLife",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        "amount": { "kind": "integer", "value": 1 },
    }]);
    assert!(
        rule_is_executable(&rule),
        "opaque IDs permit prefix relationships"
    );
    let source = card_instance("player-0", &definition("source", "Artifact", "{0}"), 1);
    let mut stack = stack_object("player-0", source, rule);
    stack.decisions.insert("cards".into(), json!([]));
    stack.targets.insert(
        "cards:1".into(),
        TargetRef::Permanent {
            instance_id: "selected-sibling".into(),
        },
    );
    assert!(!super::target_belongs_to_declaration(
        &stack, "cards", "cards:1"
    ));
    assert!(super::target_belongs_to_declaration(
        &stack, "cards:1", "cards:1"
    ));
    assert!(super::declared_unselected_target_ids(&stack).contains("cards"));
    // A real member of this declaration invalidates an empty receipt.
    stack.targets.insert(
        "cards:0".into(),
        TargetRef::Permanent {
            instance_id: "selected-parent".into(),
        },
    );
    assert!(!super::declared_unselected_target_ids(&stack).contains("cards"));
}

#[test]
fn absent_or_nonempty_receipt_cannot_prove_an_empty_selection() {
    let mut rule = target_rule("cards", json!({"kind":"permanents","where":null}));
    rule["declaration"]["decisions"][0]["minimum"] = json!(0);
    let source = card_instance("player-0", &definition("source", "Artifact", "{0}"), 1);
    let mut stack = stack_object("player-0", source, rule);
    assert!(!super::declared_unselected_target_ids(&stack).contains("cards"));
    stack.decisions.insert("cards".into(), json!(["unmatched"]));
    assert!(!super::declared_unselected_target_ids(&stack).contains("cards"));
    stack.decisions.insert("cards".into(), json!([]));
    assert!(super::declared_unselected_target_ids(&stack).contains("cards"));
}
