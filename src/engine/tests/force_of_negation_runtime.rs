use super::*;

fn parsed_counter_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Force of Negation".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}{U}".to_string()),
        oracle_text: Some(
            "If it's not your turn, you may exile a blue card from your hand rather than pay this spell's mana cost.\nCounter target noncreature spell. If that spell is countered this way, exile it instead of putting it into its owner's graveyard."
                .to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });
    let rule = parsed.abilities[1]
        .rule
        .clone()
        .expect("native replacement-counter grammar emits a rule");
    assert!(rule_is_executable(&rule));
    rule
}

fn target_spell(cant_be_countered: bool) -> StackObject {
    StackObject {
        id: "stack:target".to_string(),
        controller: "player-1".to_string(),
        card: test_instance(
            "target-spell",
            test_definition("target-spell", "Instant"),
            "player-1",
        ),
        cant_be_countered,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn counter_stack_object(rule: &Value) -> StackObject {
    StackObject {
        id: "stack:counter".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("counter", test_definition("counter", "Instant"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetSpell".to_string(),
            TargetRef::StackObject {
                stack_id: "stack:target".to_string(),
            },
        )]),
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn successful_counter_replaces_the_graveyard_destination_with_exile() {
    let rule = parsed_counter_rule();
    let mut engine = test_engine(2);
    engine.state.stack.push(target_spell(false));

    engine
        .execute_effects(
            &rule["effects"],
            &counter_stack_object(&rule),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("replacement counter resolves");

    assert!(engine.state.stack.is_empty());
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "target-spell")
    );
    assert_eq!(
        engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "spellCountered")
            .count(),
        1
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn uncounterable_spell_stays_on_stack_without_the_exile_flag_or_counter_event() {
    let rule = parsed_counter_rule();
    let mut engine = test_engine(2);
    engine.state.stack.push(target_spell(true));

    engine
        .execute_effects(
            &rule["effects"],
            &counter_stack_object(&rule),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("an uncounterable spell prevents the counter");

    let target = engine
        .state
        .stack
        .iter()
        .find(|object| object.id == "stack:target")
        .expect("uncounterable target remains on the stack");
    assert!(!target.exile_on_leave_stack);
    assert!(
        engine
            .state
            .events
            .iter()
            .all(|event| event.kind != "spellCountered")
    );
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "counterPrevented")
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn declined_unless_payment_does_not_mark_an_uncounterable_spell_for_later_exile() {
    let mut engine = test_engine(2);
    engine.state.stack.push(target_spell(true));
    let effects = crate::engine::resolution_costs::optional_cost_program(
        "payment0",
        json!({"kind":"controllerOf","object":{"kind":"chosenTarget","id":"targetSpell"}}),
        json!({"kind":"payMana","manaCost":"{3}"}),
        vec![],
        vec![
            json!({"kind":"counterStackObject","object":{"kind":"chosenTarget","id":"targetSpell"},"exileInstead":true}),
        ],
    );
    let counter = counter_stack_object(&json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": effects.clone(),
    }));

    engine
        .execute_effects(
            &json!(effects),
            &counter,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut WardDecisionProvider { pay: false },
        )
        .expect("declining payment attempts the counter");

    let target = engine
        .state
        .stack
        .iter()
        .find(|object| object.id == "stack:target")
        .expect("uncounterable target remains on the stack");
    assert!(!target.exile_on_leave_stack);
    assert!(
        engine
            .state
            .events
            .iter()
            .all(|event| event.kind != "spellCountered")
    );

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the unmarked spell resolves normally");
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "target-spell")
    );
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .all(|card| card.instance_id != "target-spell")
    );
}
