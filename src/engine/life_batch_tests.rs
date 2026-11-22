use super::super::*;
use super::*;
use rand::RngCore;

fn definition(id: &str, rules: Vec<Value>) -> CardDefinition {
    serde_json::from_value(
        json!({"id":id,"name":id,"typeLine":"Enchantment","manaCost":"{0}","rules":rules}),
    )
    .unwrap()
}
fn game() -> (GameEngine, StackObject) {
    let filler = definition("filler", vec![]);
    let mut engine = GameEngine::new(
        GameSetup {
            players: (0..4)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 12],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        10943,
    )
    .unwrap();
    let mut card = card_instance("p0", &definition("source", vec![]), 201);
    engine.prepare_battlefield_entry_identity(&mut card);
    engine.state.players[0].battlefield.push(card.clone());
    let object = StackObject {
        id: "life-source-stack".into(),
        controller: "p0".into(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    (engine, object)
}
fn loss(amount: Value) -> Value {
    json!({"kind":"loseLife","player":{"kind":"opponentsOf","player":{"kind":"abilityController"}},"amount":amount})
}

#[test]
fn ring_combat_trigger_uses_generic_life_loss_and_obeys_player_policies() {
    for level in [3, 4] {
        let (mut engine, source) = game();
        engine.state.rule_modifiers.push(json!({
            "kind": "ringEmblem", "playerId": "p0",
            "bearerId": source.card.instance_id, "level": level,
        }));
        engine.state.rule_modifiers.push(json!({
            "kind": "lifeTotalCannotChange", "playerId": "p2",
        }));
        engine.state.players[3].has_lost = true;
        engine.enqueue_combat_damage_to_player_triggers(&source.card.instance_id, "p1", 1);
        if level == 3 {
            assert!(engine.state.stack.is_empty());
            continue;
        }
        let rule = engine
            .state
            .stack
            .last()
            .unwrap()
            .ability_rule
            .as_ref()
            .unwrap();
        assert_eq!(
            rule["effects"],
            json!([loss(json!({"kind":"integer","value":3}))])
        );
        assert!(rule_is_executable(rule));
        engine.resolve_top_stack(&mut NoChoice).unwrap();
        assert_eq!(
            engine
                .state
                .players
                .iter()
                .map(|p| p.life)
                .collect::<Vec<_>>(),
            vec![20, 17, 20, 20]
        );
        assert_eq!(
            engine
                .state
                .events
                .iter()
                .filter(|e| e.kind == "lifeLost")
                .count(),
            1
        );
    }
}
fn run(engine: &mut GameEngine, object: &StackObject, amount: Value) -> Result<bool, EngineError> {
    engine.execute_direct_life_effect(
        &loss(amount),
        object,
        &mut BTreeMap::new(),
        &BTreeMap::new(),
    )
}
fn watcher(engine: &mut GameEngine, condition: Option<Value>) {
    let mut rule = json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"lifeLost","player":{"kind":"opponentsOf","player":{"kind":"controllerOf","object":{"kind":"self"}}}},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]});
    if let Some(condition) = condition {
        rule["condition"] = condition;
    }
    assert!(triggered_rule_supported(&rule));
    let mut card = card_instance("p0", &definition("watcher", vec![rule]), 202);
    engine.prepare_battlefield_entry_identity(&mut card);
    engine.state.players[0].battlefield.push(card);
}
#[test]
fn life_batch_commits_all_totals_and_records_before_intervening_if_callbacks() {
    let (mut e, s) = game();
    let conditions = json!({"kind":"and","operands":[{"kind":"compare","operator":"==","left":{"kind":"countEventsThisTurn","event":"lifeLost"},"right":3},{"kind":"compare","operator":"==","left":{"kind":"lifeLostThisTurn","player":{"kind":"controllerOf","object":{"kind":"self"}}},"right":2}]});
    watcher(&mut e, Some(conditions));
    let watcher_id = e.state.players[0].battlefield[1].instance_id.clone();
    e.change_permanent_controller(&watcher_id, "p3").unwrap();
    e.state.players[3].battlefield[0].definition.rules[0]["event"]["player"] =
        json!({"kind":"eachPlayer"});
    run(&mut e, &s, json!(2)).unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        vec![20, 18, 18, 18]
    );
    assert_eq!(e.state.stack.len(), 3);
    let last_record = e
        .state
        .events
        .iter()
        .rposition(|event| event.kind == "lifeLost")
        .unwrap();
    let first_trigger = e
        .state
        .events
        .iter()
        .position(|event| event.kind == "triggeredAbilityPutOnStack")
        .unwrap();
    assert!(last_record < first_trigger);
}
#[test]
fn loss_lock_and_eliminated_players_are_filtered_without_loss_events() {
    let (mut e, s) = game();
    watcher(&mut e, None);
    e.state
        .rule_modifiers
        .push(json!({"kind":"lifeTotalCannotChange","playerId":"p2"}));
    e.state.players[3].has_lost = true;
    run(&mut e, &s, json!(2)).unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        vec![20, 18, 20, 20]
    );
    assert_eq!(e.state.stack.len(), 1);
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|x| x.kind == "lifeLost")
            .count(),
        1
    );
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|x| x.kind == "lifeChangePrevented")
            .count(),
        1
    );
}
#[test]
fn zero_and_negative_direct_loss_have_no_events_or_observers() {
    for amount in [0, -2] {
        let (mut e, s) = game();
        watcher(&mut e, None);
        let before = e.state.clone();
        run(&mut e, &s, json!(amount)).unwrap();
        assert_eq!(e.state, before);
    }
}
#[test]
fn missing_numeric_amount_is_not_defaulted_to_zero() {
    let (mut e, s) = game();
    let before = e.state.clone();
    let error = run(
        &mut e,
        &s,
        json!({"kind":"decisionResult","decisionId":"missing"}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("unresolved life amount"));
    assert_eq!(e.state, before);
}
#[test]
fn readonly_loss_preflight_checks_all_totals_before_mutating_any_player() {
    let (mut e, s) = game();
    e.state.players[3].life = i32::MIN + 1;
    let before = e.state.clone();
    let error = run(&mut e, &s, json!(2)).unwrap_err();
    assert!(error.to_string().contains("total overflow"));
    assert_eq!(e.state, before);
}
fn attach_loss_multiplier(e: &mut GameEngine, id: &str, ordinal: usize, factor: i32) {
    let rule = json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[{"kind":"multiplyAttachedPlayerLifeLoss","factor":factor}]});
    assert!(battlefield_static_rule_supported(&rule));
    let mut card = card_instance("p0", &definition(id, vec![rule]), ordinal);
    card.attached_to = Some("p1".into());
    e.prepare_battlefield_entry_identity(&mut card);
    e.state.players[0].battlefield.push(card);
}
#[test]
fn loss_replacement_products_use_checked_arithmetic_before_commit() {
    let (mut e, s) = game();
    attach_loss_multiplier(&mut e, "double-a", 203, i32::MAX);
    attach_loss_multiplier(&mut e, "double-b", 204, 2);
    let before = e.state.clone();
    assert!(
        run(&mut e, &s, json!(1))
            .unwrap_err()
            .to_string()
            .contains("replacement amount overflow")
    );
    assert_eq!(e.state, before);
}
#[test]
fn recipients_remain_ability_controller_opponents_after_real_source_control_change() {
    let (mut e, s) = game();
    e.change_permanent_controller(&s.card.instance_id, "p1")
        .unwrap();
    assert!(
        e.state.players[1]
            .battlefield
            .iter()
            .any(|c| c.instance_id == s.card.instance_id)
    );
    run(&mut e, &s, json!(2)).unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        vec![20, 18, 18, 18]
    );
}
struct NoChoice;
impl DecisionProvider for NoChoice {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        panic!("life batch has no choice")
    }
}
#[test]
fn public_loss_refusal_restores_stack_rng_and_frames_then_retries_cleanly() {
    let (mut e, mut s) = game();
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[],"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1},loss(json!({"kind":"decisionResult","decisionId":"missing"}))]});
    s.ability_rule = Some(rule);
    e.state.stack.push(s);
    let before = e.clone();
    let mut provider = NoChoice;
    assert!(
        e.resolve_top_stack(&mut provider)
            .unwrap_err()
            .to_string()
            .contains("unresolved life amount")
    );
    assert_eq!(e.state, before.state);
    assert_eq!(e.target_numeric_contexts, before.target_numeric_contexts);
    assert_eq!(e.stack_sequence, before.stack_sequence);
    assert_eq!(e.copy_sequence, before.copy_sequence);
    let mut ar = e.rng.clone();
    let mut br = before.rng.clone();
    assert_eq!(ar.next_u64(), br.next_u64());
    assert!(!e.quantity_transaction_in_progress());
    assert!(!e.quantity_preflight_in_progress());
    e.state.stack[0].ability_rule.as_mut().unwrap()["effects"][1]["amount"] = json!(2);
    e.resolve_top_stack(&mut provider).unwrap();
    assert_eq!(
        e.state.players.iter().map(|p| p.life).collect::<Vec<_>>(),
        vec![21, 18, 18, 18]
    );
    assert!(e.state.stack.is_empty());
    assert_eq!(e.target_numeric_contexts, before.target_numeric_contexts);
}
#[test]
fn unproved_life_event_aggregation_is_capped_before_totals_change() {
    let (mut e, s) = game();
    watcher(&mut e, None);
    e.state.players[0].battlefield[1].definition.rules[0]["event"]["aggregation"] =
        json!("oneOrMore");
    let before = e.state.clone();
    assert!(
        run(&mut e, &s, json!(2))
            .unwrap_err()
            .to_string()
            .contains("shared batch event contract")
    );
    assert_eq!(e.state, before);
}

#[test]
fn player_set_uses_retained_ability_actor_after_source_leaves() {
    let (mut engine, object) = game();
    engine.exile_permanent(&object.card.instance_id).unwrap();
    run(&mut engine, &object, json!(2)).unwrap();
    assert_eq!(
        engine
            .state
            .players
            .iter()
            .map(|p| p.life)
            .collect::<Vec<_>>(),
        vec![20, 18, 18, 18]
    );
}

#[test]
fn direct_loss_defers_elimination_until_the_complete_effect_sequence_boundary() {
    let (mut engine, object) = game();
    engine.state.players[1].life = 1;
    run(&mut engine, &object, json!(2)).unwrap();
    assert_eq!(engine.state.players[1].life, -1);
    assert!(!engine.state.players[1].has_lost);
    assert!(engine.state.outcome.is_none());
    engine.gain_life("p1", 3).unwrap();
    engine.check_life_totals();
    assert_eq!(engine.state.players[1].life, 2);
    assert!(!engine.state.players[1].has_lost);
    assert!(engine.state.outcome.is_none());
}
