use super::*;

fn parsed_stack_item_counter_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Stack Counter".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some("Counter target triggered ability or colorless spell.".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("native stack-item counter grammar emits a rule");
    assert!(rule_is_executable(&rule));
    rule
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn countering_a_triggered_ability_keeps_its_source_on_the_battlefield() {
    let rule = parsed_stack_item_counter_rule();
    let mut engine = test_engine(2);
    let source = test_instance(
        "trigger-source",
        test_definition("trigger-source", "Creature"),
        "player-1",
    );
    engine.state.players[1].battlefield.push(source.clone());
    engine.state.stack.push(StackObject {
        id: "stack:trigger".to_string(),
        controller: "player-1".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(json!({
            "kind": "triggeredAbility",
            "source": { "kind": "self" },
            "event": { "kind": "permanentEntered", "object": { "kind": "self" } },
            "effects": [],
        })),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    let counter = StackObject {
        id: "stack:counter".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "stack-counter",
            test_definition("stack-counter", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetStackObject".to_string(),
            TargetRef::StackObject {
                stack_id: "stack:trigger".to_string(),
            },
        )]),
    };

    engine
        .execute_effects(
            &rule["effects"],
            &counter,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("primitive counterStackObject resolves");

    assert!(engine.state.stack.is_empty());
    assert!(
        engine.state.players[1]
            .battlefield
            .iter()
            .any(|permanent| permanent.instance_id == "trigger-source")
    );
    assert!(engine.state.players[1].graveyard.is_empty());
    let event = engine
        .state
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "spellCountered")
        .expect("counter event is recorded");
    assert_eq!(event.detail["objectKind"], "triggeredAbility");
    assert_eq!(event.detail["destination"], "counteredAbility");
}
