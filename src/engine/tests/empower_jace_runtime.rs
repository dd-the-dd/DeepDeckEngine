use super::*;

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

/// The primitive program `empower Jace N` lowers to (CR 701.71a), mirrored from
/// the Oracle lowering test. The parser refuses the keyword until the catalog
/// Jace token compiles completely, so this proves the runtime path directly.
fn empower_rule(count: i64) -> Value {
    let jace = json!({
        "kind": "and",
        "operands": [
            { "kind": "isToken" },
            { "kind": "cardTypeContains", "value": "Planeswalker" },
            { "kind": "subtypeContains", "value": "Jace" },
        ],
    });
    let controls = json!({ "kind": "controlsPermanent", "where": jace });
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
        "effects": [
            {
                "kind": "conditional",
                "condition": { "kind": "not", "operand": controls },
                "then": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": { "kind": "integer", "value": 1 },
                    "token": { "kind": "namedToken", "name": "Jace", "colors": ["Blue"] },
                }],
            },
            {
                "kind": "conditional",
                "condition": controls,
                "then": [
                    {
                        "kind": "choosePermanents",
                        "id": "effect0:empoweredJace",
                        "player": controller(),
                        "candidates": {
                            "kind": "permanents",
                            "controller": controller(),
                            "where": jace,
                        },
                        "minimum": { "kind": "integer", "value": 1 },
                        "maximum": { "kind": "integer", "value": 1 },
                    },
                    {
                        "kind": "putCounters",
                        "permanent": {
                            "kind": "decisionResult",
                            "decisionId": "effect0:empoweredJace",
                        },
                        "counter": "loyalty",
                        "count": { "kind": "integer", "value": count },
                    },
                ],
            },
        ],
    });
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn jace_token(id: &str, owner: &str, loyalty: i32) -> CardInstance {
    let mut definition = test_definition(id, "Token Planeswalker - Jace");
    definition.is_token = true;
    definition.name = "Jace".to_string();
    let mut token = test_instance(id, definition, owner);
    token.counters.insert("loyalty".to_string(), loyalty);
    token
}

fn resolve(engine: &mut GameEngine, source: &CardInstance, rule: &Value) {
    let trigger = StackObject {
        id: "stack:empower".to_string(),
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
        .expect("empower Jace resolves");
}

fn jaces(engine: &GameEngine, player: usize) -> Vec<&CardInstance> {
    engine.state.players[player]
        .battlefield
        .iter()
        .filter(|card| card.definition.name == "Jace")
        .collect()
}

fn source() -> CardInstance {
    test_instance(
        "keeper",
        test_definition("keeper", "Creature - Human Wizard"),
        "player-0",
    )
}

#[test]
fn without_a_jace_token_the_catalog_token_is_created_then_receives_the_counters() {
    let rule = empower_rule(2);
    let source = source();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone()];

    resolve(&mut engine, &source, &rule);

    let created = jaces(&engine, 0);
    assert_eq!(created.len(), 1);
    let token = created[0];
    assert!(token.definition.is_token);
    assert!(type_line_contains(
        &token.definition.type_line,
        "Planeswalker"
    ));
    assert!(type_line_contains(&token.definition.type_line, "Jace"));
    assert_eq!(token.counters.get("loyalty"), Some(&2));
    // Blue is stated by the keyword action, not by the catalog.
    assert!(token.definition.rules.iter().any(|rule| {
        rule["colorIndicator"]
            .as_array()
            .is_some_and(|colors| colors.iter().any(|color| color == "U"))
    }));
}

#[test]
fn an_existing_jace_token_is_reused_and_a_second_resolution_accumulates() {
    let rule = empower_rule(2);
    let source = source();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield =
        vec![source.clone(), jace_token("old-jace", "player-0", 3)];

    resolve(&mut engine, &source, &rule);
    assert_eq!(jaces(&engine, 0).len(), 1, "no second token is created");
    assert_eq!(jaces(&engine, 0)[0].counters.get("loyalty"), Some(&5));

    resolve(&mut engine, &source, &rule);
    assert_eq!(jaces(&engine, 0).len(), 1);
    assert_eq!(jaces(&engine, 0)[0].counters.get("loyalty"), Some(&7));
}

#[test]
fn with_several_jace_tokens_exactly_one_is_chosen_and_opponents_are_ignored() {
    let rule = empower_rule(1);
    let source = source();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        source.clone(),
        jace_token("jace-a", "player-0", 1),
        jace_token("jace-b", "player-0", 1),
    ];
    engine.state.players[1].battlefield = vec![jace_token("enemy-jace", "player-1", 4)];

    resolve(&mut engine, &source, &rule);

    let mine = jaces(&engine, 0);
    assert_eq!(mine.len(), 2);
    let total: i32 = mine
        .iter()
        .map(|card| card.counters.get("loyalty").copied().unwrap_or(0))
        .sum();
    assert_eq!(total, 3, "one chosen token gains one counter");
    assert_eq!(jaces(&engine, 1)[0].counters.get("loyalty"), Some(&4));
}

#[test]
fn an_opponents_jace_token_does_not_prevent_creating_your_own() {
    let rule = empower_rule(1);
    let source = source();
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[1].battlefield = vec![jace_token("enemy-jace", "player-1", 4)];

    resolve(&mut engine, &source, &rule);

    assert_eq!(jaces(&engine, 0).len(), 1);
    assert_eq!(jaces(&engine, 0)[0].counters.get("loyalty"), Some(&1));
    assert_eq!(jaces(&engine, 1)[0].counters.get("loyalty"), Some(&4));
}
