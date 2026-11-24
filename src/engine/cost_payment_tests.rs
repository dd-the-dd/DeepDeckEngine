use super::super::*;

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

fn instance(id: &str, definition: CardDefinition, controller: &str) -> CardInstance {
    let mut card = card_instance(controller, &definition, 0);
    card.instance_id = id.to_string();
    card.controller = controller.to_string();
    card
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
        101,
    )
    .expect("cost payment test setup is valid")
}

fn action(source_id: &str, life_payment: i32) -> LegalAction {
    LegalAction {
        id: format!("activate:{source_id}"),
        kind: ActionKind::ActivateAbility,
        player_id: "player-0".to_string(),
        label: "Activate test ability".to_string(),
        card_instance_id: Some(source_id.to_string()),
        payment_sources: Vec::new(),
        decisions: BTreeMap::from([(
            "activatedLifePayment".to_string(),
            Value::from(life_payment),
        )]),
        targets: BTreeMap::new(),
        target_order: Vec::new(),
        attacker_id: None,
        blocker_id: None,
    }
}

fn stack_object(source: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:activated-cost-test".to_string(),
        controller: source.controller.clone(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn observer(event: Value) -> CardInstance {
    let mut definition = definition("cost-observer", "Artifact");
    definition.rules = vec![json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": event,
        "effects": [{
            "kind": "putCounters",
            "permanent": { "kind": "self" },
            "counter": "charge",
            "count": { "kind": "integer", "value": 1 }
        }]
    })];
    instance("cost-observer", definition, "player-0")
}

#[test]
fn activated_life_payment_publishes_after_the_ability_is_on_the_stack() {
    let mut engine = engine();
    let rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{
            "kind": "payLife",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "amount": { "kind": "integer", "value": 1 }
        }],
        "effects": []
    });
    let mut source_definition = definition("life-cost-source", "Artifact");
    source_definition.rules = vec![rule.clone()];
    let source = instance("life-cost-source", source_definition, "player-0");
    engine.state.players[0].battlefield = vec![
        source.clone(),
        observer(json!({
            "kind": "lifeLost",
            "player": { "kind": "eachPlayer" }
        })),
    ];
    let payment = engine
        .prepare_activated_cost_payment(&action("life-cost-source", 1), &rule)
        .expect("the complete cost transaction validates before mutation");
    let payment = engine
        .commit_activated_cost_payment(&payment)
        .expect("validated life payment commits");
    assert_eq!(engine.state.players[0].life, 19);
    assert!(engine.state.stack.is_empty());
    assert!(
        !engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "lifeLost")
    );

    engine.state.stack.push(stack_object(&source, &rule));
    engine.publish_activated_cost_payment(payment);
    assert_eq!(engine.state.stack.len(), 2);
    assert_eq!(engine.state.stack[0].id, "stack:activated-cost-test");
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "lifeLost"
            && event.player_id.as_deref() == Some("player-0")
            && event.detail["amount"] == 1
            && event.detail["cause"] == "cost"
    }));
    assert!(
        !engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "lifePaid")
    );
}

#[test]
fn cant_be_sacrificed_rejects_the_transaction_before_any_mutation() {
    let mut engine = engine();
    let sacrifice_rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{
            "kind": "sacrificePermanent",
            "permanent": { "kind": "self" }
        }],
        "effects": []
    });
    let mut source_definition = definition("protected-source", "Artifact");
    source_definition.rules = vec![
        json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" }
            },
            "modifiers": [{ "kind": "cantBeSacrificed", "object": { "kind": "self" } }]
        }),
        sacrifice_rule.clone(),
    ];
    engine.state.players[0].battlefield =
        vec![instance("protected-source", source_definition, "player-0")];

    assert!(!engine.activated_sacrifice_costs_are_payable(
        &sacrifice_rule,
        "player-0",
        "protected-source",
        &BTreeMap::new(),
    ));
    assert!(
        engine
            .prepare_activated_cost_payment(&action("protected-source", 0), &sacrifice_rule)
            .is_err()
    );
    assert_eq!(engine.state.players[0].life, 20);
    assert!(engine.permanent_position("protected-source").is_some());
    assert!(engine.state.events.is_empty());
}

#[test]
fn sacrifice_event_and_trigger_survive_a_graveyard_to_exile_replacement() {
    let mut engine = engine();
    let source = instance(
        "replacement-sacrifice",
        definition("replacement-sacrifice", "Artifact"),
        "player-0",
    );
    engine.state.players[0].battlefield = vec![
        source,
        observer(json!({
            "kind": "permanentSacrificed",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } }
        })),
    ];
    engine.state.rule_modifiers.push(json!({
        "kind": "exileIfLeavesBattlefield",
        "cardInstanceId": "replacement-sacrifice"
    }));

    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .move_permanent_to_graveyard("replacement-sacrifice", "sacrificed")
        .expect("the sacrifice is replaced with exile");

    assert!(engine.permanent_position("replacement-sacrifice").is_none());
    assert!(
        engine.state.players[0]
            .exile
            .iter()
            .any(|card| card.instance_id == "replacement-sacrifice")
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "permanentSacrificed"
            && event.card_instance_id.as_deref() == Some("replacement-sacrifice")
    }));
    assert!(!engine.state.events.iter().any(|event| {
        event.kind == "permanentDied"
            && event.card_instance_id.as_deref() == Some("replacement-sacrifice")
    }));
    assert_eq!(engine.state.stack.len(), 1);
}

fn energy_rule(count: i64) -> Value {
    json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{
            "kind": "payPlayerCounters",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "counter": "energy",
            "count": { "kind": "integer", "value": count }
        }],
        "effects": []
    })
}

/// `Pay {E}{E}{E}` (Bristling Hydra): an activation the payer cannot afford
/// is neither listed nor payable; an affordable one removes exactly its
/// counters inside the cost transaction (CR 107.14, 118.3).
#[test]
fn energy_cost_refuses_an_unaffordable_activation_and_pays_an_affordable_one() {
    let mut engine = engine();
    let rule = energy_rule(3);
    let mut source_definition = definition("energy-source", "Creature - Hydra");
    source_definition.rules = vec![rule.clone()];
    engine.state.players[0].battlefield =
        vec![instance("energy-source", source_definition, "player-0")];
    engine.state.players[0]
        .counters
        .insert("energy".to_string(), 2);

    assert!(!engine.activated_nonmana_costs_are_payable(
        &rule,
        "player-0",
        "energy-source",
        &BTreeMap::new(),
    ));
    assert!(
        engine
            .prepare_activated_cost_payment(&action("energy-source", 0), &rule)
            .is_err()
    );
    assert_eq!(engine.state.players[0].counters.get("energy"), Some(&2));

    engine.state.players[0]
        .counters
        .insert("energy".to_string(), 4);
    assert!(engine.activated_nonmana_costs_are_payable(
        &rule,
        "player-0",
        "energy-source",
        &BTreeMap::new(),
    ));
    let payment = engine
        .prepare_activated_cost_payment(&action("energy-source", 0), &rule)
        .expect("four energy pay a three-energy cost");
    let committed = engine
        .commit_activated_cost_payment(&payment)
        .expect("the energy payment commits");
    engine.publish_activated_cost_payment(committed);
    assert_eq!(engine.state.players[0].counters.get("energy"), Some(&1));
}

/// `Sacrifice two other creatures` (Priest of Forgotten Gods) lowers to one
/// sacrifice per declared object `id:index`; each is paid, and one permanent
/// cannot pay both.
#[test]
fn counted_sacrifice_cost_pays_each_declared_permanent_once() {
    let mut engine = engine();
    let rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "activationCostPermanent0:0" } },
            { "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "activationCostPermanent0:1" } }
        ],
        "effects": []
    });
    let mut source_definition = definition("priest", "Creature - Human Cleric");
    source_definition.rules = vec![rule.clone()];
    engine.state.players[0].battlefield = vec![
        instance("priest", source_definition, "player-0"),
        instance(
            "bear-a",
            definition("bear-a", "Creature - Bear"),
            "player-0",
        ),
        instance(
            "bear-b",
            definition("bear-b", "Creature - Bear"),
            "player-0",
        ),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let permanent = |id: &str| TargetRef::Permanent {
        instance_id: id.to_string(),
    };

    let mut duplicate = action("priest", 0);
    duplicate.targets.insert(
        "activationCostPermanent0:0".to_string(),
        permanent("bear-a"),
    );
    duplicate.targets.insert(
        "activationCostPermanent0:1".to_string(),
        permanent("bear-a"),
    );
    assert!(
        engine
            .prepare_activated_cost_payment(&duplicate, &rule)
            .is_err()
    );

    let mut paid = action("priest", 0);
    paid.targets.insert(
        "activationCostPermanent0:0".to_string(),
        permanent("bear-a"),
    );
    paid.targets.insert(
        "activationCostPermanent0:1".to_string(),
        permanent("bear-b"),
    );
    let payment = engine
        .prepare_activated_cost_payment(&paid, &rule)
        .expect("two distinct creatures pay the counted sacrifice");
    let committed = engine
        .commit_activated_cost_payment(&payment)
        .expect("the counted sacrifice commits");
    engine.publish_activated_cost_payment(committed);
    assert!(engine.permanent_position("bear-a").is_none());
    assert!(engine.permanent_position("bear-b").is_none());
    assert!(engine.permanent_position("priest").is_some());
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "permanentSacrificed")
            .count(),
        2
    );
}

/// `Discard two cards at random` is selected by the engine while paying:
/// exactly the counted number leaves the hand, and a hand that is too small
/// refuses the activation before any mutation.
#[test]
fn random_discard_cost_discards_exactly_its_count_from_the_hand() {
    let mut engine = engine();
    let rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{
            "kind": "discardCards",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "count": { "kind": "integer", "value": 2 },
            "where": null,
            "selection": { "kind": "random" }
        }],
        "effects": []
    });
    assert!(super::random_discard_cost_supported(&rule["costs"][0]));
    let mut source_definition = definition("anarch", "Creature - Dragon");
    source_definition.rules = vec![rule.clone()];
    engine.state.players[0].battlefield = vec![instance("anarch", source_definition, "player-0")];
    engine.state.players[0].hand = vec![instance(
        "card-a",
        definition("card-a", "Instant"),
        "player-0",
    )];
    engine.state.players[0].graveyard.clear();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(!engine.activated_nonmana_costs_are_payable(
        &rule,
        "player-0",
        "anarch",
        &BTreeMap::new(),
    ));
    assert!(
        engine
            .prepare_activated_cost_payment(&action("anarch", 0), &rule)
            .is_err()
    );
    assert_eq!(engine.state.players[0].hand.len(), 1);

    engine.state.players[0].hand = ["card-a", "card-b", "card-c"]
        .into_iter()
        .map(|id| instance(id, definition(id, "Instant"), "player-0"))
        .collect();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let payment = engine
        .prepare_activated_cost_payment(&action("anarch", 0), &rule)
        .expect("three cards pay a two-card random discard");
    let committed = engine
        .commit_activated_cost_payment(&payment)
        .expect("the random discard commits");
    engine.publish_activated_cost_payment(committed);
    assert_eq!(engine.state.players[0].hand.len(), 1);
    assert_eq!(engine.state.players[0].graveyard.len(), 2);
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "cardsRandomlySelected"
            && event.detail["reason"] == "cost"
            && event.detail["cardIds"]
                .as_array()
                .is_some_and(|ids| ids.len() == 2)
    }));
}

/// `{X}, Remove X +1/+1 counters from this creature`: the declared X is the
/// counter count; a declared X larger than the counters is not payable, and
/// X = 0 removes nothing.
#[test]
fn x_counter_removal_cost_is_bound_by_the_declared_x() {
    let cost = json!({
        "kind": "removeCounters",
        "permanent": { "kind": "self" },
        "counter": "+1/+1",
        "count": { "kind": "decisionResult", "decisionId": "xValue" }
    });
    assert!(super::activated_cost_count_supported(&cost["count"]));
    let mut source = instance("ooze", definition("ooze", "Creature - Ooze"), "player-0");
    source.counters.insert("+1/+1".to_string(), 2);
    let declared = |x: i64| BTreeMap::from([("xValue".to_string(), Value::from(x))]);
    assert!(GameEngine::remove_counter_cost_has_option(&source, &cost));
    for (x, payable) in [(0, true), (2, true), (3, false)] {
        assert_eq!(
            GameEngine::remove_counter_cost_is_payable(&source, &cost, &declared(x)),
            payable,
            "X = {x}"
        );
    }
    assert_eq!(
        super::activated_cost_count(&cost["count"], &declared(2)),
        Some(2)
    );
    assert_eq!(
        super::activated_cost_count(&cost["count"], &BTreeMap::new()),
        None
    );
}

/// Nantuko Husk sacrificing itself to its own ability: the source is a new
/// object in the graveyard, so `This creature gets +2/+2` does nothing
/// (CR 400.7, 113.7a) instead of failing or touching another object.
#[test]
fn a_source_self_modification_is_a_no_op_once_the_source_left() {
    let mut engine = engine();
    let rule = json!({
        "kind": "activatedAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "sacrificePermanent", "permanent": { "kind": "chosenObject", "id": "activationCostPermanent0" } }],
        "effects": [{
            "kind": "modifyPowerToughness",
            "object": { "kind": "self" },
            "power": { "kind": "integer", "value": 2 },
            "toughness": { "kind": "integer", "value": 2 },
            "duration": { "kind": "untilEndOfCurrentTurn" }
        }]
    });
    let mut husk_definition = definition("husk", "Creature - Phyrexian Zombie Insect");
    husk_definition.rules = vec![rule.clone()];
    engine.state.players[0].battlefield = vec![instance("husk", husk_definition, "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let carried = engine.state.players[0].battlefield[0].clone();
    let mut paid = action("husk", 0);
    paid.targets.insert(
        "activationCostPermanent0".to_string(),
        TargetRef::Permanent {
            instance_id: "husk".to_string(),
        },
    );
    let payment = engine
        .prepare_activated_cost_payment(&paid, &rule)
        .expect("the source may pay its own chosen sacrifice");
    let committed = engine
        .commit_activated_cost_payment(&payment)
        .expect("the self-sacrifice commits");
    engine.publish_activated_cost_payment(committed);
    assert!(engine.permanent_position("husk").is_none());

    let modifiers_before = engine.state.rule_modifiers.len();
    let stack = stack_object(&carried, &rule);
    engine
        .execute_effects(
            &rule["effects"],
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut FirstOptionProvider,
        )
        .expect("a departed source makes the modification a no-op");
    assert_eq!(engine.state.rule_modifiers.len(), modifiers_before);
}

struct FirstOptionProvider;

impl DecisionProvider for FirstOptionProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(0)
    }
}
