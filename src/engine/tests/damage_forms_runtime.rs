//! Oracle text -> primitive damage IR -> runtime behavior for object-sourced
//! damage (CR 120.1), fight (CR 701.14) and multi-group damage recipients.
use super::*;

fn parsed_rule(card_name: &str, type_line: &str, oracle_text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("damage-forms-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{oracle_text}");
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("the spell emits a canonical rule");
    assert!(rule_is_executable(&rule), "{rule}");
    rule
}

fn creature(id: &str, owner: &str, power: &str, toughness: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature — Beast");
    definition.power = Some(power.to_string());
    definition.toughness = Some(toughness.to_string());
    test_instance(id, definition, owner)
}

/// A resolving spell (no ability kind/rule) with fixture targets; the engine
/// admits the fixture objects and designates the targets while they are legal.
fn spell(targets: &[(&str, &str)], engine: &mut GameEngine) -> StackObject {
    let mut stack_object = StackObject {
        id: "stack:damage-forms".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "damage-forms-spell",
            test_definition("damage-forms-spell", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: targets
            .iter()
            .map(|(id, instance_id)| {
                (
                    id.to_string(),
                    TargetRef::Permanent {
                        instance_id: instance_id.to_string(),
                    },
                )
            })
            .collect(),
    };
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    designate_fixture_targets(engine, &mut stack_object);
    stack_object
}

fn resolve(engine: &mut GameEngine, rule: &Value, stack_object: &StackObject) {
    execute_designated_effects(
        engine,
        &rule["effects"],
        stack_object,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the parsed damage program resolves");
}

fn damage_on(engine: &GameEngine, instance_id: &str) -> Option<i32> {
    engine
        .state
        .players
        .iter()
        .flat_map(|player| player.battlefield.iter())
        .find(|permanent| permanent.instance_id == instance_id)
        .map(|permanent| permanent.damage_marked)
}

fn declared_target_ids(rule: &Value) -> Vec<String> {
    rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|decision| decision["kind"] == "chooseTargets")
        .filter_map(|decision| decision["id"].as_str().map(str::to_string))
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn rocky_rebuke_target_source_deals_its_power_and_an_illegal_source_deals_none() {
    let rule = parsed_rule(
        "Rocky Rebuke",
        "Instant",
        "Target creature you control deals damage equal to its power to target creature an opponent controls.",
    );
    let ids = declared_target_ids(&rule);
    assert_eq!(ids.len(), 2, "{rule}");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("biter", "player-0", "3", "3")];
    engine.state.players[1].battlefield = vec![creature("bitten", "player-1", "2", "4")];
    let stack_object = spell(&[(&ids[0], "biter"), (&ids[1], "bitten")], &mut engine);
    resolve(&mut engine, &rule, &stack_object);
    assert_eq!(damage_on(&engine, "bitten"), Some(3));
    assert_eq!(damage_on(&engine, "biter"), Some(0), "a bite is one-sided");

    // CR 608.2b: the source target left before resolution; no other object
    // (not even the spell) deals the damage instead.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("biter", "player-0", "3", "3")];
    engine.state.players[1].battlefield = vec![creature("bitten", "player-1", "2", "4")];
    let stack_object = spell(&[(&ids[0], "biter"), (&ids[1], "bitten")], &mut engine);
    engine.state.players[0].battlefield.clear();
    resolve(&mut engine, &rule, &stack_object);
    assert_eq!(damage_on(&engine, "bitten"), Some(0));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn epic_confrontation_modifies_then_its_target_fights_the_second_target() {
    let rule = parsed_rule(
        "Epic Confrontation",
        "Sorcery",
        "Target creature you control gets +1/+2 until end of turn. It fights target creature you don't control. (Each deals damage equal to its power to the other.)",
    );
    let ids = declared_target_ids(&rule);
    assert_eq!(ids.len(), 2, "{rule}");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("fighter", "player-0", "2", "2")];
    engine.state.players[1].battlefield = vec![creature("foe", "player-1", "3", "5")];
    let stack_object = spell(&[(&ids[0], "fighter"), (&ids[1], "foe")], &mut engine);
    resolve(&mut engine, &rule, &stack_object);
    // The +1/+2 applies before the fight (CR 608.2c): 3 power, 4 toughness.
    assert_eq!(damage_on(&engine, "foe"), Some(3));
    assert_eq!(damage_on(&engine, "fighter"), Some(3));
}

/// Unused sibling combination: a player-set group and a controller-scoped
/// permanent collection in one damage conjunction.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn conjunction_reaches_each_opponent_and_only_their_creatures() {
    let rule = parsed_rule(
        "Ember Sweep",
        "Sorcery",
        "Ember Sweep deals 2 damage to each creature your opponents control and each opponent.",
    );
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(2), "{rule}");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("mine", "player-0", "2", "2")];
    engine.state.players[1].battlefield = vec![creature("theirs", "player-1", "2", "3")];
    let stack_object = spell(&[], &mut engine);
    resolve(&mut engine, &rule, &stack_object);
    assert_eq!(damage_on(&engine, "mine"), Some(0));
    assert_eq!(damage_on(&engine, "theirs"), Some(2));
    assert_eq!(engine.state.players[0].life, 20);
    assert_eq!(engine.state.players[1].life, 18);
}
