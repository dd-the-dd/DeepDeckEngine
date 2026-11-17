use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn lowered_attack_token_rule(quantity: i64) -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": {
            "kind": "declaredAttacker",
            "object": { "kind": "self" },
        },
        "effects": [
            {
                "kind": "createTokens",
                "controller": controller(),
                "quantity": { "kind": "integer", "value": quantity },
                "tapped": true,
                "attacking": true,
                "bind": "createdAttackers",
                "token": {
                    "types": ["Creature"],
                    "subtypes": ["Warrior"],
                    "colors": ["Red"],
                    "power": { "kind": "integer", "value": 1 },
                    "toughness": { "kind": "integer", "value": 1 },
                },
            },
            {
                "kind": "installDelayedStepTrigger",
                "step": "endStep",
                "controller": controller(),
                "captureBindings": ["createdAttackers"],
                "effects": [{
                    "kind": "sacrificePermanents",
                    "player": controller(),
                    "objects": {
                        "kind": "boundObjects",
                        "binding": "createdAttackers",
                    },
                }],
            },
        ],
    })
}

fn cast_prohibition_rule() -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "prohibitCast",
            "players": {
                "kind": "opponentsOf",
                "player": controller(),
            },
            "duringTurnOf": controller(),
        }],
    })
}

fn setup_attacker(rule: Value) -> (GameEngine, String) {
    let mut engine = test_engine(2);
    let mut definition = test_definition("attack-token-source", "Creature - Human Bard");
    definition.rules = vec![rule];
    let attacker = test_instance("attack-token-source", definition, "player-0");
    let attacker_id = attacker.instance_id.clone();
    engine.state.players[0].battlefield.push(attacker);
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: attacker_id.clone(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
    (engine, attacker_id)
}

#[test]
fn lowered_attack_trigger_creates_tapped_attackers_and_cleans_up_only_its_bound_batch() {
    let rule = lowered_attack_token_rule(2);
    assert!(rule_is_executable(&rule));
    let (mut engine, attacker_id) = setup_attacker(rule);

    let mut unrelated_warrior = test_instance(
        "unrelated-warrior",
        test_definition("unrelated-warrior", "Creature - Warrior"),
        "player-0",
    );
    unrelated_warrior.definition.is_token = true;
    let unrelated_id = unrelated_warrior.instance_id.clone();
    engine.state.players[0].battlefield.push(unrelated_warrior);

    engine.enqueue_declared_attacker_triggers(std::slice::from_ref(&attacker_id));
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("lowered attack-token trigger resolves");

    let created_ids = engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "tokenCreated")
        .filter_map(|event| event.card_instance_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(created_ids.len(), 2);
    for instance_id in &created_ids {
        let (player_index, card_index) = engine
            .permanent_position(instance_id)
            .expect("created attacker is on the battlefield");
        let token = &engine.state.players[player_index].battlefield[card_index];
        assert!(token.tapped);
        assert!(token.flags.get("attacking").copied().unwrap_or(false));
        let assignment = engine
            .state
            .combat
            .attackers
            .iter()
            .find(|assignment| assignment.attacker_id == *instance_id)
            .expect("created token is in combat");
        assert_eq!(assignment.defending_player_id, "player-1");
    }
    assert!(!engine.state.events.iter().any(|event| {
        event.kind == "abilityPerformed" && event.detail["abilityId"] == "mobilize"
    }));
    let captured = engine
        .state
        .rule_modifiers
        .iter()
        .find(|modifier| value_kind(modifier) == Some("delayedStepTrigger"))
        .expect("cleanup trigger is installed");
    assert_eq!(
        captured["capturedBindings"]["createdAttackers"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    engine.enqueue_step_triggers("endStep");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("captured cleanup trigger resolves");

    assert!(
        created_ids
            .iter()
            .all(|instance_id| engine.permanent_position(instance_id).is_none())
    );
    assert!(engine.permanent_position(&unrelated_id).is_some());
    assert!(engine.permanent_position(&attacker_id).is_some());
}

#[test]
fn delayed_cleanup_ignores_a_bound_token_that_already_left_and_keeps_unrelated_tokens() {
    let (mut engine, attacker_id) = setup_attacker(lowered_attack_token_rule(2));
    engine.enqueue_declared_attacker_triggers(std::slice::from_ref(&attacker_id));
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("attack-token trigger resolves");
    let created_ids = engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "tokenCreated")
        .filter_map(|event| event.card_instance_id.clone())
        .collect::<Vec<_>>();
    engine
        .move_permanent_to_graveyard(&created_ids[0], "destroyed")
        .expect("one created token leaves before cleanup");

    engine.enqueue_step_triggers("endStep");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("cleanup tolerates a missing bound object");

    assert!(engine.permanent_position(&created_ids[0]).is_none());
    assert!(engine.permanent_position(&created_ids[1]).is_none());
    assert!(engine.permanent_position(&attacker_id).is_some());
}

#[test]
fn delayed_cleanup_does_not_sacrifice_a_bound_token_an_opponent_now_controls() {
    let (mut engine, attacker_id) = setup_attacker(lowered_attack_token_rule(2));
    engine.enqueue_declared_attacker_triggers(std::slice::from_ref(&attacker_id));
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("attack-token trigger resolves");
    let created_ids = engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "tokenCreated")
        .filter_map(|event| event.card_instance_id.clone())
        .collect::<Vec<_>>();
    let (owner_index, token_index) = engine
        .permanent_position(&created_ids[0])
        .expect("created token exists before control changes");
    let mut stolen = engine.state.players[owner_index]
        .battlefield
        .remove(token_index);
    stolen.controller = "player-1".to_string();
    engine.state.players[1].battlefield.push(stolen);

    engine.enqueue_step_triggers("endStep");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("cleanup resolves after control changes");

    assert!(engine.permanent_position(&created_ids[0]).is_some());
    assert!(engine.permanent_position(&created_ids[1]).is_none());
}

#[test]
fn cast_prohibition_uses_its_player_and_turn_references_dynamically() {
    let rule = cast_prohibition_rule();
    assert!(rule_is_executable(&rule));
    let mut engine = test_engine(2);
    let mut definition = test_definition("cast-prohibition-source", "Creature");
    definition.rules = vec![rule];
    engine.state.players[0].battlefield.push(test_instance(
        "cast-prohibition-source",
        definition,
        "player-0",
    ));

    assert!(engine.spell_casting_prohibited("player-1"));
    assert!(!engine.spell_casting_prohibited("player-0"));

    engine.state.active_player = 1;
    assert!(!engine.spell_casting_prohibited("player-1"));
    assert!(!engine.spell_casting_prohibited("player-0"));

    engine.state.active_player = 0;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .move_permanent_to_graveyard("cast-prohibition-source", "destroyed")
        .expect("prohibition source leaves");
    assert!(!engine.spell_casting_prohibited("player-1"));
}

#[test]
fn mobilize_keyword_shell_is_not_an_executable_substitute_for_lowered_actions() {
    assert!(!rule_is_executable(&json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "mobilize", "quantity": 2 },
    })));
    assert!(effect_supported(&json!({
        "kind": "sacrificePermanents",
        "player": controller(),
        "objects": { "kind": "boundObjects", "binding": "createdBatch" },
    })));
    assert!(!effect_supported(&json!({
        "kind": "sacrificePermanents",
        "player": controller(),
        "objects": { "kind": "boundObjects" },
    })));
    assert!(!effect_supported(&json!({
        "kind": "sacrificePermanents",
        "objects": { "kind": "boundObjects", "binding": "createdBatch" },
    })));
}
