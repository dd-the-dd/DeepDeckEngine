//! Coordinated static subjects form one continuous effect over the union of
//! their populations (CR 604.1, 611.3): an object in two subjects is
//! affected once.

use super::*;

fn parse_static(name: &str, type_line: &str, oracle: &str) -> crate::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{B}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn changeling(id: &str, controller: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Shapeshifter");
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    definition.rules = vec![json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": { "kind": "changeling" },
    })];
    test_instance(id, definition, controller)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_member_of_two_coordinated_subjects_gets_the_bonus_once() {
    let parsed = parse_static(
        "Coordinated lord probe",
        "Creature - Human",
        "Goblins you control and Elementals you control get +1/+1.",
    );
    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("the coordinated static ability emits a rule");
    let modifiers = rule["modifiers"].as_array().expect("static modifiers");
    assert_eq!(modifiers.len(), 1, "one modifier over the union: {rule}");
    assert_eq!(modifiers[0]["objects"]["where"]["kind"], "or");
    assert!(rule_is_executable(&rule));

    let mut lord_definition = test_definition("coordinated-lord", "Creature - Human");
    lord_definition.power = Some("1".to_string());
    lord_definition.toughness = Some("1".to_string());
    lord_definition.rules = vec![rule];
    let lord = test_instance("coordinated-lord", lord_definition, "player-0");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![lord, changeling("both-types", "player-0")];
    engine.state.players[1].battlefield = vec![changeling("opposing-both-types", "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    let own = engine.state.players[0].battlefield[1].clone();
    let opposing = engine.state.players[1].battlefield[0].clone();
    let lord = engine.state.players[0].battlefield[0].clone();
    assert_eq!(
        engine.current_power(&own),
        3,
        "a Goblin Elemental gets +1/+1 once"
    );
    assert_eq!(engine.current_power(&opposing), 2);
    assert_eq!(engine.current_power(&lord), 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn overlapping_coordinated_subjects_that_cannot_merge_are_refused() {
    // Death Baron: `other` excludes the source from one branch only, so the
    // branches cannot share one collection; the populations overlap
    // (a Skeleton Zombie), and separate modifiers would double the bonus.
    let death_baron = parse_static(
        "Death Baron",
        "Creature - Zombie Skeleton Wizard",
        "Skeletons you control and other Zombies you control get +1/+1 and have deathtouch.",
    );
    assert_ne!(death_baron.status, "canonical");
    // One subject has a controller relation and the other does not.
    let mixed_controllers = parse_static(
        "Static frame probe",
        "Creature - Goblin Shaman",
        "Goblins you control and Elementals have menace.",
    );
    assert_ne!(mixed_controllers.status, "canonical");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn disjoint_controller_populations_lower_separately() {
    let parsed = parse_static(
        "Static frame probe",
        "Creature - Human",
        "Creatures you control and creatures your opponents control have flying.",
    );
    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0].rule.clone().expect("a static rule");
    let modifiers = rule["modifiers"].as_array().expect("static modifiers");
    assert_eq!(modifiers.len(), 2, "{rule}");
    assert_ne!(
        modifiers[0]["objects"]["controller"],
        modifiers[1]["objects"]["controller"]
    );
}
