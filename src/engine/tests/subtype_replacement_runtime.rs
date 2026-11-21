use super::*;

const DOCTOR: &str = "Doctor";

fn subtype_effect(mode: &str, subtypes: &[&str], duration: &str) -> Value {
    json!({
        "kind": "modifySubtypeSet",
        "object": { "kind": "chosenTarget", "id": "changed" },
        "subtypeClass": "creature",
        "mode": mode,
        "subtypes": subtypes,
        "duration": { "kind": duration },
    })
}

fn setup(type_line: &str) -> (GameEngine, StackObject) {
    let mut engine = test_engine(2);
    let mut definition = test_definition("changed", type_line);
    definition.power = Some("2".to_string());
    definition.toughness = Some("2".to_string());
    let source = test_instance("changed", definition, "player-0");
    engine.state.players[0].battlefield.push(source.clone());
    let stack = StackObject {
        id: "stack:subtype-change".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "changed".to_string(),
            TargetRef::Permanent {
                instance_id: "changed".to_string(),
            },
        )]),
    };
    (engine, stack)
}

fn resolve(engine: &mut GameEngine, stack: &StackObject, effects: Vec<Value>) {
    for effect in &effects {
        assert!(effect_supported(effect), "{effect}");
    }
    engine
        .execute_effects(
            &Value::Array(effects),
            stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("subtype change resolves");
}

fn is_subtype(engine: &GameEngine, subtype: &str) -> bool {
    let permanent = &engine.state.players[0].battlefield[0];
    engine.permanent_matches_effective_filter(
        permanent,
        &json!({ "kind": "subtypeContains", "value": subtype }),
    )
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn replacement_drops_the_old_creature_types_and_ends_at_cleanup() {
    let (mut engine, stack) = setup("Creature - Elf Warrior");
    assert!(is_subtype(&engine, "Elf"));
    resolve(
        &mut engine,
        &stack,
        vec![subtype_effect(
            "replace",
            &[DOCTOR],
            "untilEndOfCurrentTurn",
        )],
    );
    assert!(is_subtype(&engine, DOCTOR));
    assert!(!is_subtype(&engine, "Elf"));
    assert!(!is_subtype(&engine, "Warrior"));
    // The card types are untouched: only the creature subtype set changed.
    assert!(
        engine.state.players[0].battlefield[0]
            .definition
            .type_line
            .starts_with("Creature")
    );

    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("cleanup");
    assert!(is_subtype(&engine, "Elf"));
    assert!(is_subtype(&engine, "Warrior"));
    assert!(!is_subtype(&engine, DOCTOR));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn addition_keeps_the_old_creature_types() {
    let (mut engine, stack) = setup("Creature - Elf");
    resolve(
        &mut engine,
        &stack,
        vec![subtype_effect("add", &["Avatar"], "permanent")],
    );
    assert!(is_subtype(&engine, "Avatar"));
    assert!(is_subtype(&engine, "Elf"));
    assert!(!is_subtype(&engine, DOCTOR));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn later_changes_follow_timestamp_order() {
    let (mut engine, stack) = setup("Creature - Elf");
    resolve(
        &mut engine,
        &stack,
        vec![
            subtype_effect("add", &["Avatar"], "permanent"),
            subtype_effect("replace", &[DOCTOR], "permanent"),
            subtype_effect("add", &["Zombie"], "permanent"),
        ],
    );
    assert!(is_subtype(&engine, DOCTOR));
    assert!(is_subtype(&engine, "Zombie"));
    assert!(!is_subtype(&engine, "Avatar"));
    assert!(!is_subtype(&engine, "Elf"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_object_without_a_creature_type_cannot_gain_a_creature_subtype() {
    let (mut engine, stack) = setup("Artifact");
    resolve(
        &mut engine,
        &stack,
        vec![subtype_effect("replace", &[DOCTOR], "permanent")],
    );
    assert!(!is_subtype(&engine, DOCTOR));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_runtime_contract_requires_a_registered_subtype_list() {
    for invalid in [
        json!({ "kind": "modifySubtypeSet", "object": { "kind": "self" }, "subtypeClass": "creature", "mode": "replace", "duration": { "kind": "permanent" } }),
        subtype_effect("replace", &[], "permanent"),
        subtype_effect("add", &["Forest"], "permanent"),
        subtype_effect("replace", &[DOCTOR], "untilNextTurn"),
        json!({ "kind": "modifySubtypeSet", "object": { "kind": "self" }, "subtypeClass": "creature", "mode": "all", "subtypes": ["Elf"], "duration": { "kind": "permanent" } }),
    ] {
        assert!(!effect_supported(&invalid), "{invalid}");
    }
}
