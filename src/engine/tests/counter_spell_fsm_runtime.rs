use super::*;

fn parsed_counter_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Negate".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some("Counter target noncreature spell.".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    let rule = parsed.abilities[0]
        .rule
        .clone()
        .expect("native counter grammar emits a rule");
    assert!(rule_is_executable(&rule));
    rule
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn parsed_simple_counter_moves_the_chosen_noncreature_spell_to_its_owners_graveyard() {
    let rule = parsed_counter_rule();
    let mut engine = test_engine(2);
    engine.state.stack.push(StackObject {
        id: "stack:instant".to_string(),
        controller: "player-1".to_string(),
        card: test_instance(
            "instant-spell",
            test_definition("instant-spell", "Instant"),
            "player-1",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    });
    let counter = StackObject {
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
                stack_id: "stack:instant".to_string(),
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
        .expect("primitive counterSpell resolves");

    assert!(engine.state.stack.is_empty());
    assert!(
        engine.state.players[1]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "instant-spell")
    );
}
