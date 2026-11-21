//! Source combat restrictions compiled from Oracle text and executed by the
//! engine's blocking and attacking legality (CR 508.1c, 509.1b, 611.3a).
use super::*;

fn definition(id: &str, type_line: &str, power: &str, rules: Vec<Value>) -> CardDefinition {
    let creature = type_line.contains("Creature");
    CardDefinition {
        id: id.to_string(),
        is_commander: false,
        is_game_piece: false,
        is_sideboard: false,
        is_token: false,
        mana_cost: "{0}".to_string(),
        name: id.to_string(),
        power: creature.then(|| power.to_string()),
        rules,
        toughness: creature.then(|| "2".to_string()),
        type_line: type_line.to_string(),
    }
}

fn instance(
    id: &str,
    type_line: &str,
    power: &str,
    rules: Vec<Value>,
    owner: &str,
) -> CardInstance {
    let definition = definition(id, type_line, power, rules);
    let mut instance = card_instance(owner, &definition, 0);
    instance.instance_id = id.to_string();
    instance
}

/// The rules the Oracle compiler produces for a printed line; each must also
/// be accepted by the engine's own support predicate.
fn compiled_rules(oracle: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Restricted".to_string(),
        type_line: "Creature - Human".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:?}",
        parsed.diagnostics
    );
    let rules = parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.clone().expect("canonical ability has a rule"))
        .collect::<Vec<_>>();
    assert!(rules.iter().all(rule_is_executable), "{oracle}: {rules:?}");
    rules
}

fn attacking(engine: &mut GameEngine, attacker: &CardInstance) {
    engine.state.combat.attackers.push(DeclaredAttacker {
        attacker_id: attacker.instance_id.clone(),
        defender: TargetRef::Player {
            player_id: "player-1".to_string(),
        },
        defending_player_id: "player-1".to_string(),
    });
}

fn blocker(id: &str, power: &str, rules: Vec<Value>) -> CardInstance {
    instance(id, "Creature - Soldier", power, rules, "player-1")
}

fn flying() -> Value {
    json!({ "kind": "flying" })
}

#[test]
fn power_qualified_blockers_are_prohibited_by_the_shared_query() {
    let mut engine = test_engine(2);
    let rules =
        compiled_rules("This creature can't be blocked by creatures with power 3 or greater.");
    let attacker = instance("dodger", "Creature - Human", "2", rules, "player-0");
    attacking(&mut engine, &attacker);
    engine.state.players[0].battlefield = vec![attacker.clone()];
    assert!(engine.can_block(&blocker("small", "2", Vec::new()), &attacker));
    assert!(!engine.can_block(&blocker("big", "3", Vec::new()), &attacker));
    assert!(!engine.can_block(&blocker("bigger", "5", Vec::new()), &attacker));
}

#[test]
fn power_ceiling_and_subtype_blocker_queries_share_the_same_modifier() {
    let mut engine = test_engine(2);
    let attacker = instance(
        "aspirant",
        "Creature - Human",
        "2",
        compiled_rules("This creature can't be blocked by creatures with power 2 or less."),
        "player-0",
    );
    attacking(&mut engine, &attacker);
    engine.state.players[0].battlefield = vec![attacker.clone()];
    assert!(!engine.can_block(&blocker("small", "2", Vec::new()), &attacker));
    assert!(engine.can_block(&blocker("big", "3", Vec::new()), &attacker));

    let crawler = instance(
        "crawler",
        "Creature - Human",
        "2",
        compiled_rules("This creature can't be blocked by Walls."),
        "player-0",
    );
    attacking(&mut engine, &crawler);
    engine.state.players[0].battlefield.push(crawler.clone());
    let wall = instance("wall", "Creature - Wall", "0", Vec::new(), "player-1");
    assert!(!engine.can_block(&wall, &crawler));
    assert!(engine.can_block(&blocker("soldier", "0", Vec::new()), &crawler));
}

#[test]
fn except_by_allows_only_the_queried_creatures() {
    let mut engine = test_engine(2);
    let attacker = instance(
        "tracer",
        "Creature - Human",
        "2",
        compiled_rules("This creature can't be blocked except by creatures with flying or reach."),
        "player-0",
    );
    attacking(&mut engine, &attacker);
    engine.state.players[0].battlefield = vec![attacker.clone()];
    assert!(!engine.can_block(&blocker("ground", "2", Vec::new()), &attacker));
    assert!(engine.can_block(&blocker("flyer", "2", vec![flying()]), &attacker));
    assert!(engine.can_block(
        &blocker("spider", "2", vec![json!({ "kind": "reach" })]),
        &attacker
    ));
}

#[test]
fn a_blocking_prohibition_with_an_attacker_query_restricts_only_the_source() {
    let mut engine = test_engine(2);
    let source = blocker(
        "orgg",
        "2",
        compiled_rules("This creature can't block creatures with power 3 or greater."),
    );
    let small = instance("small", "Creature - Human", "2", Vec::new(), "player-0");
    let big = instance("big", "Creature - Human", "3", Vec::new(), "player-0");
    attacking(&mut engine, &small);
    attacking(&mut engine, &big);
    engine.state.players[1].battlefield = vec![source.clone()];
    assert!(engine.can_block(&source, &small));
    assert!(!engine.can_block(&source, &big));
    // Another creature of the same controller is unaffected.
    assert!(engine.can_block(&blocker("other", "2", Vec::new()), &big));
}

#[test]
fn as_long_as_gates_the_restriction_on_the_current_board() {
    let mut engine = test_engine(2);
    let attacker = instance(
        "island-hopper",
        "Creature - Human",
        "2",
        compiled_rules("This creature can't be blocked as long as you control an Island."),
        "player-0",
    );
    attacking(&mut engine, &attacker);
    engine.state.players[0].battlefield = vec![attacker.clone()];
    let plain = blocker("plain", "2", Vec::new());
    assert!(engine.can_block(&plain, &attacker));
    engine.state.players[0].battlefield.push(instance(
        "island",
        "Basic Land - Island",
        "0",
        Vec::new(),
        "player-0",
    ));
    assert!(!engine.can_block(&plain, &attacker));

    // The same gate on a qualified blocker clause (a previously unused pair).
    let mut engine = test_engine(2);
    let attacker = instance(
        "selective",
        "Creature - Human",
        "2",
        compiled_rules(
            "This creature can't be blocked by creatures with power 3 or greater as long as you control an Island.",
        ),
        "player-0",
    );
    attacking(&mut engine, &attacker);
    engine.state.players[0].battlefield = vec![attacker.clone()];
    let big = blocker("big", "4", Vec::new());
    assert!(engine.can_block(&big, &attacker));
    engine.state.players[0].battlefield.push(instance(
        "island",
        "Basic Land - Island",
        "0",
        Vec::new(),
        "player-0",
    ));
    assert!(!engine.can_block(&big, &attacker));
    assert!(engine.can_block(&blocker("small", "1", Vec::new()), &attacker));
}

#[test]
fn a_conditional_block_prohibition_toggles_the_cant_block_marker() {
    let mut engine = test_engine(2);
    let source = blocker(
        "guard",
        "2",
        compiled_rules("This creature can't block as long as you control an Island."),
    );
    engine.state.players[1].battlefield = vec![source.clone()];
    assert!(!engine.permanent_has_keyword(&engine.state.players[1].battlefield[0], "cantBlock"));
    engine.state.players[1].battlefield.push(instance(
        "island",
        "Basic Land - Island",
        "0",
        Vec::new(),
        "player-1",
    ));
    assert!(engine.permanent_has_keyword(&engine.state.players[1].battlefield[0], "cantBlock"));
}

#[test]
fn at_most_one_blocker_stops_a_second_declaration() {
    let mut engine = test_engine(2);
    let attacker = instance(
        "wurm",
        "Creature - Wurm",
        "2",
        compiled_rules("This creature can't be blocked by more than one creature."),
        "player-0",
    );
    attacking(&mut engine, &attacker);
    engine.state.players[0].battlefield = vec![attacker.clone()];
    engine.state.players[1].battlefield = vec![
        blocker("first", "2", Vec::new()),
        blocker("second", "2", Vec::new()),
    ];
    let attackers = vec!["wurm".to_string()];
    let mut blocks = BTreeMap::new();
    let first_choices = engine.legal_blocker_actions(1, &attackers, &BTreeSet::new(), &blocks);
    assert!(
        first_choices
            .iter()
            .any(|action| action.blocker_id.as_deref() == Some("second"))
    );

    blocks.insert("wurm".to_string(), vec!["first".to_string()]);
    let used = BTreeSet::from(["first".to_string()]);
    let later = engine.legal_blocker_actions(1, &attackers, &used, &blocks);
    assert!(
        later.iter().all(|action| action.blocker_id.is_none()),
        "no further block is offered: {later:?}"
    );
}

#[test]
fn attacks_each_combat_if_able_is_required_only_while_its_condition_holds() {
    let mut engine = test_engine(2);
    let source = instance(
        "berserker",
        "Creature - Human",
        "2",
        compiled_rules(
            "This creature attacks each combat if able as long as you control an Island.",
        ),
        "player-0",
    );
    engine.state.players[0].battlefield = vec![source.clone()];
    assert!(!engine.permanent_has_self_static_modifier(&source, "attackEachCombatIfAble"));
    engine.state.players[0].battlefield.push(instance(
        "island",
        "Basic Land - Island",
        "0",
        Vec::new(),
        "player-0",
    ));
    assert!(engine.permanent_has_self_static_modifier(&source, "attackEachCombatIfAble"));

    let plain = instance(
        "plain-berserker",
        "Creature - Human",
        "2",
        compiled_rules("This creature attacks each combat if able."),
        "player-1",
    );
    engine.state.players[1].battlefield = vec![plain.clone()];
    assert!(engine.permanent_has_self_static_modifier(&plain, "attackEachCombatIfAble"));
}

#[test]
fn unrepresentable_neighbours_stay_explicitly_unsupported() {
    for oracle in [
        "This creature can block an additional creature each combat.",
        "This creature can't attack alone.",
        "This creature can't be blocked except by three or more creatures.",
    ] {
        let parsed = parse_oracle_card(OracleCardParseRequest {
            card_name: "Restricted".to_string(),
            type_line: "Creature - Human".to_string(),
            mana_cost: Some("{2}".to_string()),
            oracle_text: Some(oracle.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        });
        assert_ne!(parsed.status, "canonical", "{oracle}");
    }
}
