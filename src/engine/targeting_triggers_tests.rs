use super::super::*;
use super::{became_target_event_supported, newly_targeted_identities};

struct FirstDecisionProvider;

impl DecisionProvider for FirstDecisionProvider {
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
        is_commander: false,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        mana_cost: "{0}".to_string(),
        power: None,
        toughness: None,
        rules: Vec::new(),
    }
}

fn engine() -> GameEngine {
    let filler = definition("filler", "Basic Land - Plains");
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
        109,
    )
    .expect("targeting trigger test setup is valid")
}

fn watcher_rule() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "becameTarget",
            "object": {
                "kind": "union",
                "sets": [
                    {
                        "kind": "permanents",
                        "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "where": { "kind": "cardTypeContains", "value": "Creature" },
                    },
                    {
                        "kind": "spells",
                        "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "where": { "kind": "cardTypeContains", "value": "Creature" },
                    },
                ],
            },
            "targetingStackObject": {
                "kind": "stackItems",
                "controller": {
                    "kind": "opponentsOf",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
                "where": {
                    "kind": "or",
                    "operands": [
                        { "kind": "isSpell" },
                        { "kind": "isActivatedAbility" },
                        { "kind": "isTriggeredAbility" },
                    ],
                },
            },
        },
        "effects": [{
            "kind": "drawCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": { "kind": "integer", "value": 1 },
        }],
    })
}

fn stack_object(
    id: &str,
    controller: &str,
    type_line: &str,
    ability_kind: Option<&str>,
) -> StackObject {
    StackObject {
        id: id.to_string(),
        controller: controller.to_string(),
        card: card_instance(controller, &definition(id, type_line), 0),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: ability_kind.map(str::to_string),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn install_watcher(engine: &mut GameEngine) {
    let mut watcher = definition("target-watcher", "Creature - Warrior");
    watcher.rules.push(watcher_rule());
    engine.state.players[0]
        .battlefield
        .push(card_instance("player-0", &watcher, 1));
}

#[test]
fn validates_generic_selector_shape_and_preserves_self_shape() {
    assert_eq!(
        became_target_event_supported(&watcher_rule()["event"]),
        Some(true)
    );
    assert_eq!(
        became_target_event_supported(&json!({
            "kind": "becameTarget",
            "object": { "kind": "self" },
            "stackObjectKind": "spell",
        })),
        Some(true)
    );
}

#[test]
fn opponent_spell_targeting_controlled_permanents_triggers_once_per_distinct_object() {
    let mut engine = engine();
    install_watcher(&mut engine);
    let mut creature_ids = Vec::new();
    for index in 0..2 {
        let mut creature = definition(&format!("creature-{index}"), "Creature - Bear");
        creature.power = Some("2".to_string());
        creature.toughness = Some("2".to_string());
        let permanent = card_instance("player-0", &creature, index + 2);
        creature_ids.push(permanent.instance_id.clone());
        engine.state.players[0].battlefield.push(permanent);
    }
    engine.state.stack.push(stack_object(
        "stack:opponent-spell",
        "player-1",
        "Instant",
        None,
    ));
    let targets = BTreeMap::from([
        (
            "first".to_string(),
            TargetRef::Permanent {
                instance_id: creature_ids[0].clone(),
            },
        ),
        (
            "duplicate".to_string(),
            TargetRef::Permanent {
                instance_id: creature_ids[0].clone(),
            },
        ),
        (
            "second".to_string(),
            TargetRef::Permanent {
                instance_id: creature_ids[1].clone(),
            },
        ),
    ]);

    engine
        .enqueue_targeting_triggers("stack:opponent-spell", &targets)
        .expect("supported target cost programs");

    let triggers = engine
        .state
        .stack
        .iter()
        .filter(|object| object.ability_kind.as_deref() == Some("triggeredAbility"))
        .collect::<Vec<_>>();
    assert_eq!(triggers.len(), 2);
    assert!(triggers.iter().all(|trigger| {
        trigger.decisions["targetingStackId"] == "stack:opponent-spell"
            && trigger.decisions["targetingControllerId"] == "player-1"
            && trigger.decisions["targetedObjectKind"] == "permanent"
            && trigger.decisions["targetedObjectControllerId"] == "player-0"
    }));
}

#[test]
fn allied_targeting_source_does_not_trigger() {
    let mut engine = engine();
    install_watcher(&mut engine);
    let creature = definition("friendly-creature", "Creature - Bear");
    let permanent = card_instance("player-0", &creature, 2);
    let permanent_id = permanent.instance_id.clone();
    engine.state.players[0].battlefield.push(permanent);
    engine.state.stack.push(stack_object(
        "stack:friendly-spell",
        "player-0",
        "Instant",
        None,
    ));

    engine
        .enqueue_targeting_triggers(
            "stack:friendly-spell",
            &BTreeMap::from([(
                "target".to_string(),
                TargetRef::Permanent {
                    instance_id: permanent_id,
                },
            )]),
        )
        .expect("supported target cost programs");

    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn activated_and_triggered_abilities_targeting_a_controlled_creature_spell_trigger() {
    for ability_kind in ["activatedAbility", "triggeredAbility"] {
        let mut engine = engine();
        install_watcher(&mut engine);
        engine.state.stack.push(stack_object(
            "stack:creature-spell",
            "player-0",
            "Creature - Bear",
            None,
        ));
        let targeting_id = format!("stack:{ability_kind}");
        engine.state.stack.push(stack_object(
            &targeting_id,
            "player-1",
            "Creature - Wizard",
            Some(ability_kind),
        ));

        engine
            .enqueue_targeting_triggers(
                &targeting_id,
                &BTreeMap::from([(
                    "target".to_string(),
                    TargetRef::StackObject {
                        stack_id: "stack:creature-spell".to_string(),
                    },
                )]),
            )
            .expect("supported target cost programs");

        let triggers = engine
            .state
            .stack
            .iter()
            .filter(|object| {
                object.ability_kind.as_deref() == Some("triggeredAbility")
                    && object.decisions.get("targetingStackId")
                        == Some(&Value::String(targeting_id.clone()))
            })
            .count();
        assert_eq!(triggers, 1, "{ability_kind} should trigger the watcher");
    }
}

#[test]
fn noncreature_target_is_a_near_miss() {
    let mut engine = engine();
    install_watcher(&mut engine);
    let artifact = definition("noncreature-artifact", "Artifact");
    let permanent = card_instance("player-0", &artifact, 2);
    let permanent_id = permanent.instance_id.clone();
    engine.state.players[0].battlefield.push(permanent);
    engine.state.stack.push(stack_object(
        "stack:artifact-targeting-spell",
        "player-1",
        "Instant",
        None,
    ));

    engine
        .enqueue_targeting_triggers(
            "stack:artifact-targeting-spell",
            &BTreeMap::from([(
                "target".to_string(),
                TargetRef::Permanent {
                    instance_id: permanent_id,
                },
            )]),
        )
        .expect("supported target cost programs");

    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn retargeting_reports_only_new_distinct_identities() {
    let first = TargetRef::Permanent {
        instance_id: "first".to_string(),
    };
    let second = TargetRef::Permanent {
        instance_id: "second".to_string(),
    };
    let previous = BTreeMap::from([
        ("a".to_string(), first.clone()),
        ("b".to_string(), second.clone()),
    ]);
    let current = BTreeMap::from([("a".to_string(), second.clone()), ("b".to_string(), second)]);

    assert!(newly_targeted_identities(&previous, &current).is_empty());

    let third = TargetRef::Permanent {
        instance_id: "third".to_string(),
    };
    let current = BTreeMap::from([
        ("a".to_string(), third.clone()),
        ("b".to_string(), third.clone()),
    ]);
    assert_eq!(
        newly_targeted_identities(&previous, &current),
        BTreeMap::from([("a".to_string(), third)])
    );
}

#[test]
fn activated_cost_targets_are_not_published_as_effect_targets() {
    let rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{
            "kind": "tap",
            "object": { "kind": "chosenTarget", "id": "costPermanent" },
        }],
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [
                { "id": "costPermanent", "kind": "chooseTargets" },
                { "id": "effectPermanent", "kind": "chooseTargets" },
            ],
        },
        "effects": [],
    });
    let targets = BTreeMap::from([
        (
            "costPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: "cost".to_string(),
            },
        ),
        (
            "effectPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: "effect".to_string(),
            },
        ),
    ]);

    assert_eq!(
        GameEngine::activated_effect_targets(&rule, &targets),
        BTreeMap::from([(
            "effectPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: "effect".to_string(),
            },
        )])
    );
}

#[test]
fn stack_item_candidates_resolve_opponents_of_the_ability_controller() {
    let mut engine = engine();
    engine.state.stack.push(stack_object(
        "stack:controlled-spell",
        "player-1",
        "Creature - Wizard",
        None,
    ));
    engine.state.stack.push(stack_object(
        "stack:opposing-spell",
        "player-0",
        "Creature - Bear",
        None,
    ));

    let candidates = engine
        .target_candidates(
            "player-1",
            &json!({
                "kind": "stackItems",
                "controller": {
                    "kind": "opponentsOf",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
                "where": {
                    "kind": "and",
                    "operands": [
                        { "kind": "isSpell" },
                        { "kind": "cardTypeContains", "value": "Creature" },
                    ],
                },
            }),
        )
        .expect("stackItems candidates are supported");

    assert_eq!(
        candidates,
        vec![TargetRef::StackObject {
            stack_id: "stack:opposing-spell".to_string(),
        }]
    );
}

#[test]
fn ward_and_generic_tax_are_each_enqueued_once_for_duplicate_target_slots() {
    let mut engine = engine();
    let mut warded = definition("warded-creature", "Creature - Bear");
    warded.rules.push(json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": {
            "kind": "ward",
            "cost": { "kind": "payMana", "manaCost": "{1}" },
        },
    }));
    let warded = card_instance("player-0", &warded, 2);
    let warded_id = warded.instance_id.clone();
    engine.state.players[0].battlefield.push(warded);

    let mut mariner = definition("tax-source", "Creature - Shapeshifter");
    mariner.rules.push(json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "becameTarget",
            "object": {
                "kind": "permanents",
                "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                "where": null,
            },
            "targetingStackObject": {
                "kind": "stackItems",
                "controller": {
                    "kind": "opponentsOf",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
                "where": null,
            },
        },
        "effects": [{
            "kind":"conditional",
            "condition":{"kind":"objectMatchesFilter","object":{"kind":"triggeringStackObject"},"where":null},
            "then":crate::engine::resolution_costs::optional_cost_program(
                "payment0",
                json!({"kind":"controllerOf","object":{"kind":"triggeringStackObject"}}),
                json!({"kind":"payMana","manaCost":"{1}"}),vec![],
                vec![json!({"kind":"counterStackObject","object":{"kind":"triggeringStackObject"}})],
            ),"else":[],
        }],
    }));
    engine.state.players[0]
        .battlefield
        .push(card_instance("player-0", &mariner, 3));
    engine.state.stack.push(stack_object(
        "stack:taxed-spell",
        "player-1",
        "Instant",
        None,
    ));
    let target = TargetRef::Permanent {
        instance_id: warded_id,
    };

    engine
        .enqueue_targeting_triggers(
            "stack:taxed-spell",
            &BTreeMap::from([
                ("first".to_string(), target.clone()),
                ("duplicate".to_string(), target),
            ]),
        )
        .expect("supported target cost programs");

    let tax_triggers = engine
        .state
        .stack
        .iter()
        .filter(|object| {
            object.ability_kind.as_deref() == Some("triggeredAbility")
                && object
                    .ability_rule
                    .as_ref()
                    .is_some_and(|rule| rule_contains_kind(rule, "tryPayCost"))
        })
        .count();
    assert_eq!(tax_triggers, 2);
}

#[test]
fn triggered_ability_targets_are_committed_before_resolution_and_dispatch_target_events() {
    let mut engine = engine();
    install_watcher(&mut engine);
    let creature = definition("trigger-target", "Creature - Bear");
    let permanent = card_instance("player-0", &creature, 2);
    let permanent_id = permanent.instance_id.clone();
    engine.state.players[0].battlefield.push(permanent);

    let mut targeting = stack_object(
        "stack:targeting-trigger",
        "player-1",
        "Creature - Wizard",
        Some("triggeredAbility"),
    );
    targeting.ability_rule = Some(json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "spellCast", "anyPlayer": true, "where": null },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetCreature",
                "kind": "chooseTargets",
                "candidates": {
                    "kind": "permanents",
                    "controller": {
                        "kind": "opponentsOf",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    },
                    "where": { "kind": "subtypeContains", "value": "Bear" },
                },
                "minimum": 1,
                "maximum": 1,
            }],
        },
        "effects": [],
    }));
    engine.state.stack.push(targeting);

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .commit_pending_triggered_targets(&mut FirstDecisionProvider)
        .expect("trigger targets are committed");

    let targeting = engine
        .state
        .stack
        .iter()
        .find(|object| object.id == "stack:targeting-trigger")
        .expect("targeting trigger remains on stack");
    assert_eq!(
        targeting.targets.get("targetCreature"),
        Some(&TargetRef::Permanent {
            instance_id: permanent_id,
        })
    );
    assert_eq!(
        targeting.decisions.get("triggerTargetsCommitted"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        engine
            .state
            .stack
            .iter()
            .filter(|object| object.ability_kind.as_deref() == Some("triggeredAbility"))
            .count(),
        2
    );

    engine
        .commit_pending_triggered_targets(&mut FirstDecisionProvider)
        .expect("committed targets are not chosen twice");
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "triggeredAbilityTargetsChosen")
            .count(),
        1
    );
}

#[test]
fn triggered_target_commitment_reaches_quiescence_after_a_target_event_cascade() {
    let mut engine = engine();
    let mut watcher = definition("declarative-watcher", "Creature - Warrior");
    let mut rule = watcher_rule();
    rule["declaration"] = json!({
        "kind": "castingDeclaration",
        "decisions": [{
            "id": "followupTarget",
            "kind": "chooseTargets",
            "minimum": 1,
            "maximum": 1,
            "candidates": {
                "kind": "permanents",
                "controller": {
                    "kind": "opponentsOf",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                },
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            },
        }],
    });
    watcher.rules.push(rule);
    engine.state.players[0]
        .battlefield
        .push(card_instance("player-0", &watcher, 1));

    let controlled = definition("controlled-target", "Creature - Bear");
    engine.state.players[0]
        .battlefield
        .push(card_instance("player-0", &controlled, 2));
    let opposing = definition("opposing-target", "Creature - Bear");
    engine.state.players[1]
        .battlefield
        .push(card_instance("player-1", &opposing, 2));

    let mut initial = stack_object(
        "stack:initial-targeting-trigger",
        "player-1",
        "Creature - Wizard",
        Some("triggeredAbility"),
    );
    initial.ability_rule = Some(json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "spellCast", "anyPlayer": true, "where": null },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "initialTarget",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "controller": {
                        "kind": "opponentsOf",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    },
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
            }],
        },
        "effects": [],
    }));
    engine.state.stack.push(initial);

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .commit_pending_triggered_targets(&mut FirstDecisionProvider)
        .expect("the initial and newly-created trigger both commit targets");

    let declared = engine
        .state
        .stack
        .iter()
        .filter(|object| {
            object.ability_kind.as_deref() == Some("triggeredAbility")
                && object.ability_rule.as_ref().is_some_and(|rule| {
                    rule["declaration"]["decisions"]
                        .as_array()
                        .is_some_and(|decisions| !decisions.is_empty())
                })
        })
        .collect::<Vec<_>>();
    assert_eq!(declared.len(), 2);
    assert!(declared.iter().all(|object| !object.targets.is_empty()));
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "triggeredAbilityTargetsChosen")
            .count(),
        2
    );
}

#[test]
fn precommitted_triggered_targets_are_not_chosen_or_dispatched_twice() {
    let mut engine = engine();
    install_watcher(&mut engine);
    let creature = definition("precommitted-target", "Creature - Bear");
    let permanent = card_instance("player-0", &creature, 2);
    let permanent_id = permanent.instance_id.clone();
    engine.state.players[0].battlefield.push(permanent);

    let mut targeting = stack_object(
        "stack:precommitted-trigger",
        "player-1",
        "Creature - Wizard",
        Some("triggeredAbility"),
    );
    targeting.ability_rule = Some(json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "spellCast", "anyPlayer": true, "where": null },
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetCreature",
                "kind": "chooseTargets",
                "candidates": { "kind": "permanents" },
                "minimum": 1,
                "maximum": 1,
            }],
        },
        "effects": [],
    }));
    targeting.targets.insert(
        "targetCreature".to_string(),
        TargetRef::Permanent {
            instance_id: permanent_id.clone(),
        },
    );
    engine.state.stack.push(targeting);

    engine
        .commit_pending_triggered_targets(&mut FirstDecisionProvider)
        .expect("precommitted targets require no new decision");

    let targeting = engine
        .state
        .stack
        .iter()
        .find(|object| object.id == "stack:precommitted-trigger")
        .expect("precommitted trigger remains on the stack");
    assert_eq!(
        targeting.targets.get("targetCreature"),
        Some(&TargetRef::Permanent {
            instance_id: permanent_id,
        })
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "triggeredAbilityTargetsChosen")
            .count(),
        0
    );
    assert_eq!(engine.state.stack.len(), 1);
}

/// `When this creature becomes the target of a spell or ability, ...`: the
/// generic pattern with the source as the targeted object and a stack object
/// of any controller (CR 115.1).
fn source_watcher_rule(controller: Option<Value>) -> Value {
    let mut stack = json!({
        "kind": "stackItems",
        "where": {
            "kind": "or",
            "operands": [
                { "kind": "isSpell" },
                { "kind": "isActivatedAbility" },
                { "kind": "isTriggeredAbility" },
            ],
        },
    });
    if let Some(controller) = controller {
        stack["controller"] = controller;
    }
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "becameTarget",
            "object": { "kind": "self" },
            "targetingStackObject": stack,
        },
        "effects": [{
            "kind": "drawCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": { "kind": "integer", "value": 1 },
        }],
    })
}

fn install_source_watcher(engine: &mut GameEngine, controller: Option<Value>) -> String {
    let mut watcher = definition("source-watcher", "Creature - Illusion");
    watcher.rules.push(source_watcher_rule(controller));
    let permanent = card_instance("player-0", &watcher, 1);
    let id = permanent.instance_id.clone();
    engine.state.players[0].battlefield.push(permanent);
    id
}

fn target_with(engine: &mut GameEngine, stack_id: &str, controller: &str, target: &str) {
    engine
        .state
        .stack
        .push(stack_object(stack_id, controller, "Instant", None));
    engine
        .enqueue_targeting_triggers(
            stack_id,
            &BTreeMap::from([(
                "target".to_string(),
                TargetRef::Permanent {
                    instance_id: target.to_string(),
                },
            )]),
        )
        .expect("supported target cost programs");
}

fn triggered_count(engine: &GameEngine) -> usize {
    engine
        .state
        .stack
        .iter()
        .filter(|object| object.ability_kind.as_deref() == Some("triggeredAbility"))
        .count()
}

#[test]
fn the_source_observer_triggers_for_a_stack_object_of_any_controller() {
    assert_eq!(
        became_target_event_supported(&source_watcher_rule(None)["event"]),
        Some(true)
    );
    for controller in ["player-0", "player-1"] {
        let mut engine = engine();
        let watcher = install_source_watcher(&mut engine, None);
        target_with(&mut engine, "stack:spell", controller, &watcher);
        // One trigger: the historical self path does not also publish the
        // generic form.
        assert_eq!(triggered_count(&engine), 1, "{controller}");
    }
}

#[test]
fn the_source_observer_reads_the_stack_controller_relation() {
    let opponents = json!({
        "kind": "opponentsOf",
        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
    });
    let mut engine = engine();
    let watcher = install_source_watcher(&mut engine, Some(opponents));
    target_with(&mut engine, "stack:own-spell", "player-0", &watcher);
    assert_eq!(triggered_count(&engine), 0, "own spell is a near miss");
    target_with(&mut engine, "stack:opponent-spell", "player-1", &watcher);
    assert_eq!(triggered_count(&engine), 1);
}

#[test]
fn another_targeted_permanent_is_a_near_miss_for_the_source_observer() {
    let mut engine = engine();
    install_source_watcher(&mut engine, None);
    let other = card_instance("player-0", &definition("other", "Creature - Bear"), 2);
    let other_id = other.instance_id.clone();
    engine.state.players[0].battlefield.push(other);
    target_with(&mut engine, "stack:spell", "player-1", &other_id);
    assert_eq!(triggered_count(&engine), 0);
}
