//! Real Oracle text -> primitive IR -> engine behavior for object references:
//! the permanent an Aura or Equipment is attached to (CR 301.5a, 303.4a) and
//! the controller or owner of a referenced object (CR 108.3, 109.4), read as
//! last known information once that object changed zones (CR 608.2h).
use super::*;

fn parsed_rules(type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: "Reference Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(parsed.status, "canonical", "{oracle}");
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn spell_rule(oracle: &str) -> Value {
    parsed_rules("Instant", oracle)
        .into_iter()
        .next()
        .expect("one spell ability")
}

fn target_id(rule: &Value) -> String {
    rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string()
}

fn stack_object(
    id: &str,
    controller: &str,
    card: CardInstance,
    rule: &Value,
    targets: BTreeMap<String, TargetRef>,
) -> StackObject {
    StackObject {
        id: id.to_string(),
        controller: controller.to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

fn resolve(engine: &mut GameEngine, rule: &Value, stack: &StackObject) {
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the parsed program resolves");
}

/// A creature owned by `owner` and controlled by `controller`.
fn creature(id: &str, owner: &str, controller: &str) -> CardInstance {
    let mut card = test_instance(id, test_definition(id, "Creature - Bear"), owner);
    card.controller = controller.to_string();
    card
}

/// Resolves `oracle` against one creature target owned by player-1 and
/// controlled by player-2: the related player must be read from the
/// targeted incarnation after the spell moved it.
fn resolve_against_borrowed_creature(oracle: &str) -> GameEngine {
    let rule = spell_rule(oracle);
    let mut engine = test_engine(3);
    engine.state.players[2]
        .battlefield
        .push(creature("borrowed-bear", "player-1", "player-2"));
    let stack = stack_object(
        "stack:reference-probe",
        "player-0",
        test_instance("probe", test_definition("probe", "Instant"), "player-0"),
        &rule,
        BTreeMap::from([(
            target_id(&rule),
            TargetRef::Permanent {
                instance_id: "borrowed-bear".to_string(),
            },
        )]),
    );
    resolve(&mut engine, &rule, &stack);
    assert!(
        engine.permanent_position("borrowed-bear").is_none(),
        "the target changed zones before its related player was read"
    );
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn destroyed_target_controller_is_last_known_controller() {
    let hands = |engine: &GameEngine| {
        engine
            .state
            .players
            .iter()
            .map(|p| p.hand.len())
            .collect::<Vec<_>>()
    };
    let before = hands(&test_engine(3));
    let engine =
        resolve_against_borrowed_creature("Destroy target creature. Its controller draws a card.");
    let after = hands(&engine);
    assert_eq!(
        after[2],
        before[2] + 1,
        "the controller when destroyed draws"
    );
    assert_eq!(after[1], before[1], "the owner is not the controller");
    assert_eq!(after[0], before[0]);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn exiled_target_owner_and_controller_stay_distinct() {
    let engine =
        resolve_against_borrowed_creature("Exile target creature. Its owner gains 4 life.");
    assert_eq!(engine.state.players[1].life, 24, "the owner gains life");
    assert_eq!(engine.state.players[2].life, 20);
    let engine = resolve_against_borrowed_creature(
        "Exile target creature. That creature's controller loses life equal to its power.",
    );
    assert_eq!(
        engine.state.players[2].life, 18,
        "last known controller and power"
    );
    assert_eq!(engine.state.players[1].life, 20);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn countered_spell_controller_is_read_from_its_counter_event() {
    let rule = spell_rule("Counter target spell. Its controller mills two cards.");
    let mut engine = test_engine(2);
    let library_before = engine.state.players[1].library.len();
    let mut opposing_spell = stack_object(
        "stack:opposing-spell",
        "player-1",
        test_instance(
            "opposing-spell",
            test_definition("opposing-spell", "Sorcery"),
            "player-1",
        ),
        &json!({}),
        BTreeMap::new(),
    );
    opposing_spell.ability_rule = None;
    engine.state.stack.push(opposing_spell);
    let mut counter = stack_object(
        "stack:counter-probe",
        "player-0",
        test_instance(
            "counter-probe",
            test_definition("counter-probe", "Instant"),
            "player-0",
        ),
        &rule,
        BTreeMap::from([(
            target_id(&rule),
            TargetRef::StackObject {
                stack_id: "stack:opposing-spell".to_string(),
            },
        )]),
    );
    counter.ability_kind = Some("spellAbility".to_string());
    resolve(&mut engine, &rule, &counter);
    assert!(
        engine.state.stack.is_empty(),
        "the target spell was countered"
    );
    assert_eq!(engine.state.players[1].library.len(), library_before - 2);
    assert_eq!(engine.state.players[0].graveyard.len(), 0);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn enchanted_creature_is_the_permanent_the_aura_is_attached_to() {
    let rules = parsed_rules(
        "Enchantment — Aura",
        "Enchant creature\n{1}{B}: Enchanted creature gets +2/+2 until end of turn.",
    );
    let activated = rules
        .iter()
        .find(|rule| rule["kind"] == "activatedAbility")
        .expect("the activated ability");
    let mut engine = test_engine(2);
    let mut aura = test_instance(
        "aura",
        test_definition("aura", "Enchantment - Aura"),
        "player-0",
    );
    aura.attached_to = Some("enchanted-bear".to_string());
    engine.state.players[0].battlefield.push(aura.clone());
    engine.state.players[1]
        .battlefield
        .push(creature("enchanted-bear", "player-1", "player-1"));
    engine.state.players[1]
        .battlefield
        .push(creature("other-bear", "player-1", "player-1"));
    let mut stack = stack_object(
        "stack:aura-ability",
        "player-0",
        aura,
        activated,
        BTreeMap::new(),
    );
    stack.ability_kind = Some("activated".to_string());
    resolve(&mut engine, activated, &stack);
    let power = |engine: &GameEngine, id: &str| {
        let (player, index) = engine.permanent_position(id).expect("on the battlefield");
        engine.current_power(&engine.state.players[player].battlefield[index])
    };
    assert_eq!(power(&engine, "enchanted-bear"), 4);
    assert_eq!(power(&engine, "other-bear"), 2);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn defending_player_reads_the_attack_triggering_player() {
    let rules = parsed_rules(
        "Creature \u{2014} Leviathan",
        "Whenever this creature attacks, defending player mills two cards.",
    );
    let mut engine = test_engine(3);
    let library = |engine: &GameEngine, index: usize| engine.state.players[index].library.len();
    let before = [library(&engine, 1), library(&engine, 2)];
    let attacker = creature("leviathan", "player-0", "player-0");
    engine.state.players[0].battlefield.push(attacker.clone());
    let mut stack = stack_object(
        "stack:attack-trigger",
        "player-0",
        attacker,
        &rules[0],
        BTreeMap::new(),
    );
    stack.ability_kind = Some("triggered".to_string());
    stack
        .decisions
        .insert("triggeringPlayerId".to_string(), json!("player-2"));
    resolve(&mut engine, &rules[0], &stack);
    assert_eq!(
        library(&engine, 2),
        before[1] - 2,
        "the attacked player mills"
    );
    assert_eq!(library(&engine, 1), before[0]);
}
