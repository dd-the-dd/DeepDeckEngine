use super::*;

fn source_with_pending_ability() -> GameEngine {
    let mut source_definition = test_definition("source-lki", "Creature");
    source_definition.power = Some("1".to_string());
    source_definition.toughness = Some("1".to_string());
    let source = test_instance("source-lki-instance", source_definition, "player-0");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(source);
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    // The pending ability captures its source as the admitted incarnation.
    let source = engine.state.players[0]
        .battlefield
        .last()
        .cloned()
        .expect("source is on the battlefield");
    engine.state.stack.push(StackObject {
        id: "stack:source-lki".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "effects": [],
        })),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    engine
}

fn mutate_source(engine: &mut GameEngine) {
    let (player_index, card_index) = engine
        .permanent_position("source-lki-instance")
        .expect("source is on the battlefield");
    engine.state.players[player_index].battlefield[card_index]
        .counters
        .insert("+1/+1".to_string(), 2);
}

fn pending_power(engine: &GameEngine) -> Option<i32> {
    let stack_object = engine.state.stack.first().expect("pending source ability");
    engine.runtime_numeric_expression(
        &json!({ "kind": "powerOf", "object": { "kind": "self" } }),
        stack_object,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
}

#[test]
fn live_source_property_uses_mutated_battlefield_value() {
    let mut engine = source_with_pending_ability();
    mutate_source(&mut engine);

    assert_eq!(pending_power(&engine), Some(3));
}

#[test]
fn graveyard_departures_capture_mutation_for_destroy_and_sacrifice() {
    for reason in ["destroyed", "sacrificed"] {
        let mut engine = source_with_pending_ability();
        mutate_source(&mut engine);

        engine
            .move_permanent_to_graveyard("source-lki-instance", reason)
            .expect("source leaves for the graveyard");

        assert_eq!(pending_power(&engine), Some(3), "reason={reason}");
    }
}

#[test]
fn blink_same_instance_id_keeps_the_departed_objects_lki() {
    let mut engine = source_with_pending_ability();
    mutate_source(&mut engine);
    engine
        .exile_permanent("source-lki-instance")
        .expect("source is exiled");
    let mut returned = engine.state.players[0]
        .exile
        .pop()
        .expect("exiled source exists");
    returned.counters.insert("+1/+1".to_string(), 6);
    engine.state.players[0].battlefield.push(returned);

    assert_eq!(pending_power(&engine), Some(3));
}

#[test]
fn bounce_captures_mutated_source_lki() {
    let mut engine = source_with_pending_ability();
    mutate_source(&mut engine);

    engine
        .return_permanent_to_hand("source-lki-instance")
        .expect("source returns to hand");

    assert_eq!(pending_power(&engine), Some(3));
}

#[test]
fn library_move_captures_mutated_source_lki() {
    let mut engine = source_with_pending_ability();
    mutate_source(&mut engine);

    engine
        .put_permanent_on_owner_library("source-lki-instance", "top")
        .expect("source moves to its owner's library");

    assert_eq!(pending_power(&engine), Some(3));
}
