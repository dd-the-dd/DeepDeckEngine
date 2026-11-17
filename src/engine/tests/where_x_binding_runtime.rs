use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn integer_value_expression(value: i64) -> Value {
    json!({ "kind": "integer", "value": value })
}

fn bound() -> Value {
    json!({ "kind": "boundValue", "id": "whereX" })
}

fn bind(value: Value) -> Value {
    json!({ "kind": "bind", "id": "whereX", "value": value })
}

fn gain_life(amount: Value) -> Value {
    json!({ "kind": "gainLife", "player": controller(), "amount": amount })
}

fn lose_life(amount: Value) -> Value {
    json!({ "kind": "loseLife", "player": controller(), "amount": amount })
}

fn source() -> CardInstance {
    test_instance(
        "binder",
        test_definition("binder", "Creature - Elf"),
        "player-0",
    )
}

/// Runs the IR a `where X is EXPR` sentence lowers to: one `bind` before the
/// sentence's actions, then actions that read `boundValue whereX` (CR 107.3).
fn resolve(engine: &mut GameEngine, source: &CardInstance, effects: Vec<Value>) {
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
        "effects": effects,
    });
    assert!(rule_is_executable(&rule), "{rule:#}");
    let trigger = StackObject {
        id: "stack:where-x".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effects(
            &rule["effects"],
            &trigger,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the where-X sentence resolves");
}

#[test]
fn the_definition_is_evaluated_once_and_every_action_reads_the_same_value() {
    // `You gain X life and lose X life, where X is your life total.` A second
    // evaluation after the gain would read 40 and leave the player at 0.
    let source = source();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone()];
    resolve(
        &mut engine,
        &source,
        vec![
            bind(json!({ "kind": "lifeTotal", "player": controller() })),
            gain_life(bound()),
            lose_life(bound()),
        ],
    );
    assert_eq!(engine.state.players[0].life, 20);
}

#[test]
fn counting_names_among_permanents_binds_the_number_of_distinct_names() {
    let source = source();
    let mut engine = test_engine(2);
    let land = |id: &str, name: &str| {
        let mut definition = test_definition(id, "Basic Land - Forest");
        definition.name = name.to_string();
        test_instance(id, definition, "player-0")
    };
    engine.state.players[0].battlefield = vec![
        source.clone(),
        land("land-a", "Forest"),
        land("land-b", "Forest"),
        land("land-c", "Island"),
        land("land-d", "Mountain"),
    ];
    resolve(
        &mut engine,
        &source,
        vec![
            bind(json!({
                "kind": "countDistinctPermanentNames",
                "player": controller(),
                "where": { "kind": "cardTypeContains", "value": "Land" },
            })),
            gain_life(bound()),
        ],
    );
    assert_eq!(engine.state.players[0].life, 23);
}

#[test]
fn life_gained_this_turn_composes_with_arithmetic_in_the_definition() {
    // `Gain 3 life. You gain X life, where X is one plus the amount of life
    // you gained this turn.`
    let source = source();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone()];
    resolve(
        &mut engine,
        &source,
        vec![
            gain_life(integer_value_expression(3)),
            bind(json!({
                "kind": "add",
                "left": integer_value_expression(1),
                "right": { "kind": "lifeGainedThisTurn", "player": controller() },
            })),
            gain_life(bound()),
        ],
    );
    assert_eq!(engine.state.players[0].life, 27);
}
