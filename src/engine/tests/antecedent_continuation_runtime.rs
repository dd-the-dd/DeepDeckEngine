//! Real Oracle text -> primitive IR -> engine behavior for antecedents the
//! one binding environment resolves across scopes: a labeled spell
//! paragraph that replaces the instruction before it on the same target
//! (CR 207.2c, 608.2c), and a condition that reads the spell its own
//! counter action targets (CR 601.2c).
use super::*;

fn parsed_rules(card_name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{B}".to_string()),
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

fn spell_stack(id: &str, rule: &Value, target: TargetRef) -> StackObject {
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared target")
        .to_string();
    StackObject {
        id: format!("stack:{id}"),
        controller: "player-0".to_string(),
        card: test_instance(id, test_definition(id, "Instant"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(target_id, target)]),
    }
}

const TRAGIC_SLIP: &str = "Target creature gets -1/-1 until end of turn.\nMorbid \u{2014} That creature gets -13/-13 until end of turn instead if a creature died this turn.";

/// The power of the targeted 2/2 after Tragic Slip resolves.
fn slipped_power(creature_died_this_turn: bool) -> i32 {
    let mut rules = parsed_rules("Tragic Slip", "Instant", TRAGIC_SLIP);
    assert_eq!(rules.len(), 1, "the Morbid paragraph continues the spell");
    let rule = rules.remove(0);
    let mut engine = test_engine(2);
    for id in ["bear", "victim"] {
        engine.state.players[1].battlefield.push(test_instance(
            id,
            test_definition(id, "Creature - Bear"),
            "player-1",
        ));
    }
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    if creature_died_this_turn {
        engine
            .move_permanent_to_graveyard("victim", "destroyed")
            .expect("another creature dies this turn");
    }
    let stack = spell_stack(
        "tragic-slip",
        &rule,
        TargetRef::Permanent {
            instance_id: "bear".to_string(),
        },
    );
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("Tragic Slip resolves");
    GameEngine::effective_power_toughness(&engine.state)["bear"].current_power
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn morbid_paragraph_replaces_the_shrink_on_the_same_target() {
    assert_eq!(slipped_power(false), 1, "no death: only -1/-1");
    assert_eq!(
        slipped_power(true),
        -11,
        "morbid: -13/-13 instead, never both, on the declared creature"
    );
}

/// Whether `Counter target spell if its mana value is 2 or less.` counters
/// a spell with this mana cost.
fn counters_spell_costing(mana_cost: &str) -> bool {
    let mut rules = parsed_rules(
        "Counter Probe",
        "Instant",
        "Counter target spell if its mana value is 2 or less.",
    );
    let rule = rules.remove(0);
    let mut engine = test_engine(2);
    let mut definition = test_definition("target-spell", "Sorcery");
    definition.mana_cost = mana_cost.to_string();
    engine.state.stack.push(StackObject {
        id: "stack:target-spell".to_string(),
        controller: "player-1".to_string(),
        card: test_instance("target-spell", definition, "player-1"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    let stack = spell_stack(
        "counter-probe",
        &rule,
        TargetRef::StackObject {
            stack_id: "stack:target-spell".to_string(),
        },
    );
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the counter resolves");
    engine.state.stack.is_empty()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn postposed_condition_reads_the_spell_its_own_counter_targets() {
    assert!(
        counters_spell_costing("{1}{R}"),
        "mana value 2 is countered"
    );
    assert!(
        !counters_spell_costing("{3}{R}"),
        "mana value 4 stays on the stack"
    );
}
