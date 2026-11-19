//! CR 615: `prevent` installs a shield for the turn. A finite shield is
//! reduced by the damage it prevents (CR 615.7); `all` is not. Combat-only,
//! recipient and source restrictions are read from the shield.
use super::*;

fn parsed_rule(type_line: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: "Generalized Warden".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("damage-prevention-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("primitive IR");
    assert!(rule_is_executable(&rule), "{rule}");
    rule
}

fn creature(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Test");
    definition.power = Some("2".to_string());
    definition.toughness = Some("5".to_string());
    test_instance(id, definition, controller)
}

fn engine_with_creatures() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("attacker", "player-0")];
    engine.state.players[1].battlefield = vec![creature("bystander", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

/// Resolves the parsed spell with its declared targets chosen in order.
fn resolve_spell(engine: &mut GameEngine, rule: &Value, chosen: Vec<TargetRef>) {
    let decisions = rule["declaration"]["decisions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut targets = BTreeMap::new();
    for (decision, target) in decisions.iter().zip(chosen) {
        targets.insert(decision["id"].as_str().expect("id").to_string(), target);
    }
    let stack_object = StackObject {
        id: "stack:prevention".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "prevention",
            test_definition("prevention", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    };
    engine
        .execute_effects(
            &rule["effects"],
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("prevention resolves");
}

fn player(id: &str) -> TargetRef {
    TargetRef::Player {
        player_id: id.to_string(),
    }
}

fn permanent(id: &str) -> TargetRef {
    TargetRef::Permanent {
        instance_id: id.to_string(),
    }
}

fn damage(engine: &mut GameEngine, target: TargetRef, amount: i32, source: &str, combat: bool) {
    engine
        .deal_damage_amount_with_kind_and_origin(
            target,
            amount,
            source,
            combat,
            QuantityOrigin::Effect,
            None,
        )
        .expect("damage resolves");
}

fn life(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .find(|player| player.id == id)
        .expect("player")
        .life
}

fn marked(engine: &GameEngine, id: &str) -> i32 {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|card| card.instance_id == id)
        .expect("permanent")
        .damage_marked
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn next_n_shield_prevents_up_to_n_and_is_consumed() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "Prevent the next 3 damage that would be dealt to any target this turn.",
    );
    resolve_spell(&mut engine, &rule, vec![player("player-1")]);
    let start = life(&engine, "player-1");

    damage(&mut engine, player("player-1"), 2, "attacker", false);
    assert_eq!(life(&engine, "player-1"), start, "two of three prevented");
    damage(&mut engine, player("player-1"), 2, "attacker", false);
    assert_eq!(
        life(&engine, "player-1"),
        start - 1,
        "one point remained, one point is dealt"
    );
    damage(&mut engine, player("player-1"), 2, "attacker", false);
    assert_eq!(life(&engine, "player-1"), start - 3, "the shield is spent");
    let prevented: i64 = engine
        .state
        .events
        .iter()
        .filter(|event| event.kind == "damagePrevented")
        .map(|event| event.detail["amount"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(prevented, 3);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn shield_protects_only_its_recipient_and_only_this_turn() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "Prevent the next 5 damage that would be dealt to target creature this turn.",
    );
    resolve_spell(&mut engine, &rule, vec![permanent("bystander")]);

    damage(&mut engine, permanent("attacker"), 2, "attacker", false);
    assert_eq!(marked(&engine, "attacker"), 2, "another creature is hit");
    damage(&mut engine, player("player-1"), 2, "attacker", false);
    assert_eq!(
        life(&engine, "player-1"),
        18,
        "a player is not the recipient"
    );
    damage(&mut engine, permanent("bystander"), 2, "attacker", false);
    assert_eq!(marked(&engine, "bystander"), 0);

    engine.state.turn_number += 1;
    damage(&mut engine, permanent("bystander"), 2, "attacker", false);
    assert_eq!(
        marked(&engine, "bystander"),
        2,
        "the shield ended with the turn"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn all_combat_damage_shield_leaves_noncombat_damage_and_never_runs_out() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "Prevent all combat damage that would be dealt this turn.",
    );
    resolve_spell(&mut engine, &rule, Vec::new());

    for _ in 0..3 {
        damage(&mut engine, player("player-1"), 4, "attacker", true);
    }
    assert_eq!(life(&engine, "player-1"), 20, "combat damage is prevented");
    damage(&mut engine, player("player-1"), 4, "attacker", false);
    assert_eq!(life(&engine, "player-1"), 16, "noncombat damage is not");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn source_shield_prevents_only_damage_from_the_chosen_source() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "Prevent all damage target creature would deal this turn.",
    );
    resolve_spell(&mut engine, &rule, vec![permanent("attacker")]);

    damage(&mut engine, player("player-1"), 3, "attacker", false);
    assert_eq!(life(&engine, "player-1"), 20);
    damage(&mut engine, player("player-1"), 3, "bystander", false);
    assert_eq!(life(&engine, "player-1"), 17);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn recipient_and_source_restrictions_combine() {
    let mut engine = engine_with_creatures();
    let rule = parsed_rule(
        "Instant",
        "Prevent all combat damage that would be dealt to you this turn by target creature.",
    );
    resolve_spell(&mut engine, &rule, vec![permanent("bystander")]);

    damage(&mut engine, player("player-0"), 2, "bystander", true);
    assert_eq!(life(&engine, "player-0"), 20, "that source, combat, to you");
    damage(&mut engine, player("player-0"), 2, "bystander", false);
    assert_eq!(life(&engine, "player-0"), 18, "noncombat damage is dealt");
    damage(&mut engine, player("player-0"), 2, "attacker", true);
    assert_eq!(life(&engine, "player-0"), 16, "another source is dealt");
    damage(&mut engine, player("player-1"), 2, "bystander", true);
    assert_eq!(life(&engine, "player-1"), 18, "another recipient is dealt");
}
