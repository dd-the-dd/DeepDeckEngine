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
