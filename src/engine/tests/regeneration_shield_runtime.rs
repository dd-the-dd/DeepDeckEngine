//! CR 701.15: `regenerate OBJECT` creates a one-shot shield; the next
//! destruction this turn is replaced by tapping, removing all damage and
//! removing the permanent from combat. `can't be regenerated` bypasses it.
use super::*;

fn parsed_rule(type_line: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: "Generalized Regenerator".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("regeneration-shield-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("primitive IR");
    assert!(rule_is_executable(&rule));
    rule
}

fn stack_object_for(source: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:regeneration".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn shield_count(engine: &GameEngine, instance_id: &str) -> usize {
    engine
        .state
        .rule_modifiers
        .iter()
        .filter(|modifier| {
            value_kind(modifier) == Some("regenerationShield")
                && modifier["cardInstanceId"].as_str() == Some(instance_id)
        })
        .count()
}

fn event_count(engine: &GameEngine, kind: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == kind)
        .count()
}

fn engine_with_attacking_regenerator() -> (GameEngine, StackObject, Value) {
    let rule = parsed_rule(
        "Creature - Skeleton",
        "{B}: Regenerate this creature. (The next time this creature would be destroyed this turn, instead tap it, remove it from combat, and heal all damage on it.)",
    );
    let mut definition = test_definition("skeleton", "Creature - Skeleton");
    definition.rules = vec![rule.clone()];
    let mut skeleton = test_instance("skeleton", definition, "player-0");
    skeleton.damage_marked = 2;
    let blocker = test_instance(
        "blocker",
        test_definition("blocker", "Creature - Wall"),
        "player-1",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![skeleton.clone()];
    engine.state.players[1].battlefield = vec![blocker];
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: "skeleton".to_string(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
    engine.state.combat.blockers.push(DeclaredBlocker {
        attacker_id: "skeleton".to_string(),
        blocker_id: "blocker".to_string(),
    });
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let stack_object = stack_object_for(&skeleton, &rule);
    (engine, stack_object, rule)
}

fn resolve_effects(engine: &mut GameEngine, stack_object: &StackObject, rule: &Value) {
    engine
        .execute_effects(
            &rule["effects"],
            stack_object,
            &mut BTreeMap::new(),
            &mut stack_object.decisions.clone(),
            &mut EmeritusDecisionProvider,
        )
        .expect("regeneration resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn shield_replaces_the_next_destruction_then_is_spent() {
    let (mut engine, stack_object, rule) = engine_with_attacking_regenerator();
    let effects = rule["effects"].clone();
    assert_eq!(effects[0]["kind"], "installRegenerationShield");
    resolve_effects(&mut engine, &stack_object, &rule);
    assert_eq!(shield_count(&engine, "skeleton"), 1);
    assert_eq!(
        engine.state.rule_modifiers[0]["expiresAfterTurn"],
        json!(engine.state.turn_number),
        "the shield expires with the turn"
    );

    let destroyed = engine
        .destroy_permanent_collection_from_stack(vec!["skeleton".to_string()], false, &stack_object)
        .expect("destruction resolves");
    assert!(
        destroyed.is_empty(),
        "a regenerated permanent is not destroyed"
    );
    let (player, index) = engine
        .permanent_position("skeleton")
        .expect("still on battlefield");
    let skeleton = &engine.state.players[player].battlefield[index];
    assert!(skeleton.tapped);
    assert_eq!(skeleton.damage_marked, 0);
    assert!(
        engine.state.combat.attackers.is_empty(),
        "removed from combat"
    );
    assert!(engine.state.combat.blockers.is_empty());
    assert_eq!(shield_count(&engine, "skeleton"), 0);
    assert_eq!(event_count(&engine, "permanentRegenerated"), 1);

    let destroyed = engine
        .destroy_permanent_collection_from_stack(vec!["skeleton".to_string()], false, &stack_object)
        .expect("second destruction resolves");
    assert_eq!(destroyed, ["skeleton".to_string()]);
    assert!(engine.permanent_position("skeleton").is_none());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_destruction_that_cannot_be_regenerated_ignores_the_shield() {
    let (mut engine, stack_object, rule) = engine_with_attacking_regenerator();
    resolve_effects(&mut engine, &stack_object, &rule);
    let destroyed = engine
        .destroy_permanent_collection_from_stack(vec!["skeleton".to_string()], true, &stack_object)
        .expect("destruction resolves");
    assert_eq!(destroyed, ["skeleton".to_string()]);
    assert_eq!(event_count(&engine, "permanentRegenerated"), 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn collection_regeneration_shields_every_matching_permanent_once() {
    let rule = parsed_rule("Instant", "Regenerate each creature you control.");
    assert_eq!(rule["effects"][0]["kind"], "installRegenerationShields");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        test_instance(
            "bear",
            test_definition("bear", "Creature - Bear"),
            "player-0",
        ),
        test_instance(
            "wolf",
            test_definition("wolf", "Creature - Wolf"),
            "player-0",
        ),
        test_instance("rock", test_definition("rock", "Artifact"), "player-0"),
    ];
    engine.state.players[1].battlefield = vec![test_instance(
        "enemy",
        test_definition("enemy", "Creature - Bear"),
        "player-1",
    )];
    let spell = test_instance("spell", test_definition("spell", "Instant"), "player-0");
    let stack_object = stack_object_for(&spell, &rule);
    resolve_effects(&mut engine, &stack_object, &rule);
    assert_eq!(shield_count(&engine, "bear"), 1);
    assert_eq!(shield_count(&engine, "wolf"), 1);
    assert_eq!(shield_count(&engine, "rock"), 0);
    assert_eq!(shield_count(&engine, "enemy"), 0);
}
