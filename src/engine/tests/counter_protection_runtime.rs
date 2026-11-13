use super::*;

fn counter_protection_rule(where_filter: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "cantBeCountered",
            "objects": {
                "kind": "spells",
                "controller": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "where": where_filter,
            },
        }],
    })
}

fn protection_source(id: &str) -> CardInstance {
    let mut definition = test_definition(id, "Enchantment");
    definition.rules = vec![counter_protection_rule(json!({
        "kind": "cardTypeContains",
        "value": "Creature",
    }))];
    test_instance(id, definition, "player-0")
}

fn spell(id: &str, type_line: &str, controller: &str) -> StackObject {
    StackObject {
        id: format!("stack:{id}"),
        controller: controller.to_string(),
        card: test_instance(id, test_definition(id, type_line), controller),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

#[test]
fn filtered_continuous_prohibition_protects_only_matching_controlled_spells() {
    let mut creature_engine = test_engine(2);
    creature_engine.state.players[0]
        .battlefield
        .push(protection_source("protection"));
    creature_engine
        .state
        .stack
        .push(spell("creature", "Creature - Beast", "player-0"));

    creature_engine
        .counter_stack_object("stack:creature", "counter", "player-1", false)
        .expect("counter attempt completes");

    assert_eq!(creature_engine.state.stack.len(), 1);
    assert_eq!(
        creature_engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "counterPrevented")
            .count(),
        1
    );

    let mut instant_engine = test_engine(2);
    instant_engine.state.players[0]
        .battlefield
        .push(protection_source("protection"));
    instant_engine
        .state
        .stack
        .push(spell("instant", "Instant", "player-0"));

    instant_engine
        .counter_stack_object("stack:instant", "counter", "player-1", false)
        .expect("counter attempt completes");

    assert!(instant_engine.state.stack.is_empty());
    assert_eq!(
        instant_engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "spellCountered")
            .count(),
        1
    );
}

#[test]
fn continuous_prohibition_is_evaluated_when_countering_not_when_casting() {
    let mut enters_after_cast = test_engine(2);
    enters_after_cast
        .state
        .stack
        .push(spell("early-creature", "Creature", "player-0"));
    enters_after_cast.state.players[0]
        .battlefield
        .push(protection_source("late-protection"));

    enters_after_cast
        .counter_stack_object("stack:early-creature", "counter", "player-1", false)
        .expect("late continuous effect is observed");
    assert_eq!(enters_after_cast.state.stack.len(), 1);

    let mut leaves_after_cast = test_engine(2);
    leaves_after_cast.state.players[0]
        .battlefield
        .push(protection_source("departing-protection"));
    leaves_after_cast
        .state
        .stack
        .push(spell("late-creature", "Creature", "player-0"));
    leaves_after_cast
        .move_permanent_to_graveyard("departing-protection", "destroyed")
        .expect("protection source leaves");

    leaves_after_cast
        .counter_stack_object("stack:late-creature", "counter", "player-1", false)
        .expect("departed continuous effect no longer applies");
    assert!(leaves_after_cast.state.stack.is_empty());
}

#[test]
fn selector_controller_and_existing_intrinsic_flag_remain_distinct() {
    let mut engine = test_engine(2);
    engine.state.players[0]
        .battlefield
        .push(protection_source("protection"));
    engine
        .state
        .stack
        .push(spell("opponent-creature", "Creature", "player-1"));

    engine
        .counter_stack_object("stack:opponent-creature", "counter", "player-0", false)
        .expect("opponent spell is not protected by the selector");
    assert!(engine.state.stack.is_empty());

    let mut intrinsic = spell("intrinsic", "Instant", "player-1");
    intrinsic.cant_be_countered = true;
    engine.state.stack.push(intrinsic);
    engine
        .counter_stack_object("stack:intrinsic", "counter", "player-0", false)
        .expect("intrinsic prohibition remains supported");
    assert_eq!(engine.state.stack.len(), 1);
}

#[test]
fn battlefield_spell_set_prohibition_is_not_mistaken_for_intrinsic_self_protection() {
    let mut definition = test_definition("creature-source", "Creature");
    definition.rules = vec![counter_protection_rule(json!({
        "kind": "cardTypeContains",
        "value": "Creature",
    }))];

    assert!(!spell_cant_be_countered(&definition));

    definition.rules = vec![json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "stack" },
        },
        "modifiers": [{
            "kind": "cantBeCountered",
            "object": { "kind": "self" },
        }],
    })];
    assert!(spell_cant_be_countered(&definition));
}
