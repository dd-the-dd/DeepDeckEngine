use super::super::*;
use serde_json::json;

struct NoChoices;

impl DecisionProvider for NoChoices {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(
            "this identity guard must not request a choice",
        ))
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
        power: Some("2".to_string()),
        toughness: Some("2".to_string()),
        rules: Vec::new(),
    }
}

fn position() -> (GameEngine, StackObject) {
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
        1713,
    )
    .expect("object-context setup");
    let mut source = card_instance("player-0", &definition("explorer", "Creature - Scout"), 0);
    GameEngine::assign_tracked_zone_identity(&mut source, "battlefield:original");
    engine.state.players[0].battlefield.push(source.clone());
    let stack = StackObject {
        id: "stack:object-context".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, stack)
}

#[test]
fn object_controller_tracks_live_control_without_changing_ability_controller() {
    let (mut engine, stack) = position();
    let mut source = engine.state.players[0].battlefield.remove(0);
    source.controller = "player-1".to_string();
    engine.state.players[1].battlefield.push(source);
    let bindings = BTreeMap::new();
    assert_eq!(
        engine.resolve_player(
            &json!({"kind":"controllerOf","object":{"kind":"self"}}),
            &stack,
            &bindings
        ),
        Some("player-0".to_string())
    );
    assert_eq!(
        engine.resolve_player(
            &json!({"kind":"controllerOfObject","object":{"kind":"self"}}),
            &stack,
            &bindings
        ),
        Some("player-1".to_string())
    );
    assert!(engine.runtime_condition_value(
        &json!({"kind":"objectOnBattlefield","object":{"kind":"self"}}),
        &stack,
        &bindings,
        &BTreeMap::new(),
    ));
}

#[test]
fn departed_source_keeps_controller_and_identity_after_same_id_returns() {
    let (mut engine, stack) = position();
    let mut source = engine.state.players[0].battlefield.remove(0);
    source.controller = "player-1".to_string();
    engine.state.players[1].battlefield.push(source);
    let source_id = stack.card.instance_id.clone();
    engine.state.stack.push(stack);
    engine
        .move_permanent_to_graveyard(&source_id, "sacrificed")
        .expect("normal exit captures pending source LKI");
    let stack = engine.state.stack[0].clone();
    let mut returned = engine.state.players[0]
        .graveyard
        .pop()
        .expect("owner graveyard");
    returned.controller = "player-0".to_string();
    GameEngine::assign_tracked_zone_identity(&mut returned, "battlefield:returned");
    engine.state.players[0].battlefield.push(returned);
    let bindings = BTreeMap::new();
    assert_eq!(
        engine.resolved_object_controller(&json!({"kind":"self"}), &stack, &bindings),
        Some("player-1".to_string())
    );
    assert!(!engine.resolved_object_is_on_battlefield(&json!({"kind":"self"}), &stack, &bindings));
    engine.execute_effects(&json!([{
        "kind":"conditional", "condition":{"kind":"objectOnBattlefield","object":{"kind":"self"}},
        "then":[{"kind":"putCounters","permanent":{"kind":"self"},"counter":"+1/+1","count":{"kind":"integer","value":1}}],
        "else":[],
    }]), &stack, &mut BTreeMap::new(), &mut BTreeMap::new(), &mut NoChoices)
        .expect("the old source must not receive counters through its returned physical card");
    engine
        .execute_ability_signal_effect(
            &json!({
                "kind":"emitAbilitySignal","abilityId":"explore",
                "player":{"kind":"controllerOfObject","object":{"kind":"self"}},
                "object":{"kind":"self"},
            }),
            &stack,
            &bindings,
        )
        .expect("completion uses captured object context");
    let event = engine
        .state
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "abilityPerformed")
        .expect("completion event");
    assert_eq!(event.player_id.as_deref(), Some("player-1"));
    assert_eq!(event.card_instance_id.as_deref(), Some(source_id.as_str()));
    assert_eq!(
        event.detail["object"]["zoneIdentity"],
        "battlefield:original"
    );
    assert_eq!(event.detail["object"]["lastKnown"], true);
    assert_eq!(
        engine.state.players[0].battlefield[0].counters.get("+1/+1"),
        None
    );
}

#[test]
fn a_changed_birth_marker_is_not_the_old_source_even_without_departure_flag() {
    let (mut engine, stack) = position();
    let source = &mut engine.state.players[0].battlefield[0];
    source.controller = "player-1".to_string();
    GameEngine::assign_tracked_zone_identity(source, "battlefield:other-object");
    assert_eq!(
        engine.resolved_object_controller(&json!({"kind":"self"}), &stack, &BTreeMap::new()),
        Some("player-0".to_string())
    );
    assert!(!engine.resolved_object_is_on_battlefield(
        &json!({"kind":"self"}),
        &stack,
        &BTreeMap::new()
    ));
}

#[test]
fn another_absent_object_cannot_borrow_source_lki_or_publish_a_false_signal() {
    let (mut engine, mut stack) = position();
    stack.targets.insert(
        "other".to_string(),
        TargetRef::Permanent {
            instance_id: "absent-other".to_string(),
        },
    );
    let object = json!({"kind":"chosenTarget","id":"other"});
    assert_eq!(
        engine.resolved_object_controller(&object, &stack, &BTreeMap::new()),
        None
    );
    let before = engine.state.events.len();
    assert!(engine.execute_ability_signal_effect(&json!({
        "kind":"emitAbilitySignal","abilityId":"explore","player":{"kind":"abilityController"},"object":object,
    }), &stack, &BTreeMap::new()).is_err());
    assert_eq!(engine.state.events.len(), before);
}

#[test]
fn explicit_subject_changes_event_identity_but_old_signals_remain_exact() {
    let (mut engine, mut stack) = position();
    let subject = card_instance(
        "player-1",
        &definition("other-explorer", "Creature - Scout"),
        1,
    );
    stack.targets.insert(
        "explorer".to_string(),
        TargetRef::Permanent {
            instance_id: subject.instance_id.clone(),
        },
    );
    engine.state.players[1].battlefield.push(subject.clone());
    engine.execute_ability_signal_effect(&json!({
        "kind":"emitAbilitySignal","abilityId":"explore","player":{"kind":"controllerOfObject","object":{"kind":"chosenTarget","id":"explorer"}},
        "object":{"kind":"chosenTarget","id":"explorer"},
    }), &stack, &BTreeMap::new()).expect("explicit subject");
    let event = engine.state.events.last().expect("subject event");
    assert_eq!(
        event.card_instance_id.as_deref(),
        Some(subject.instance_id.as_str())
    );
    assert_eq!(event.player_id.as_deref(), Some("player-1"));
    engine.execute_ability_signal_effect(&json!({
        "kind":"emitAbilitySignal","abilityId":"surveil","player":{"kind":"abilityController"},"payload":{"inspected":2},
    }), &stack, &BTreeMap::new()).expect("legacy signal contract");
    let event = engine.state.events.last().expect("surveil event");
    assert_eq!(
        event.card_instance_id.as_deref(),
        Some(stack.card.instance_id.as_str())
    );
    assert_eq!(event.player_id.as_deref(), Some("player-0"));
    assert_eq!(
        event.detail,
        json!({"abilityId":"surveil","payload":{"inspected":2}})
    );
}

#[test]
fn subject_reference_validation_is_generic_and_rejects_unknown_expressions() {
    assert!(player_reference_supported(
        &json!({"kind":"controllerOfObject","object":{"kind":"self"}})
    ));
    assert!(condition_supported(
        &json!({"kind":"objectOnBattlefield","object":{"kind":"chosenTarget","id":"one"}})
    ));
    assert!(!player_reference_supported(
        &json!({"kind":"controllerOfObject","object":{"kind":"unimplementedObjectContext"}})
    ));
    assert_eq!(
        ability_signals::ability_signal_effect_supported(&json!({
            "kind":"emitAbilitySignal","abilityId":"explore","player":{"kind":"abilityController"},"object":{"kind":"unknownObject"},
        })),
        Some(false)
    );
}
