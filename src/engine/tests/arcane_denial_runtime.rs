use super::*;

const ORACLE: &str = "Counter target spell. Its controller may draw up to two cards at the beginning of the next turn's upkeep.\nYou draw a card at the beginning of the next turn's upkeep.";

struct MaximumDrawProvider;

impl DecisionProvider for MaximumDrawProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        (!request.options.is_empty())
            .then_some(0)
            .ok_or_else(|| EngineError::new("test decision has no option"))
    }

    fn choose_number(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<i32, EngineError> {
        decision_number_bounds(request)
            .map(|(_, maximum)| maximum)
            .ok_or_else(|| EngineError::new("test decision is not numeric"))
    }
}

fn parsed_rules() -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Arcane Denial".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("native Arcane Denial rule"))
        .inspect(|rule| assert!(rule_is_executable(rule)))
        .collect()
}

fn stack_spell(cant_be_countered: bool) -> StackObject {
    StackObject {
        id: "stack:target-spell".to_string(),
        controller: "player-1".to_string(),
        card: test_instance(
            "target-spell",
            test_definition("target-spell", "Instant"),
            "player-1",
        ),
        cant_be_countered,
        exile_on_leave_stack: false,
        // A spell on the stack is not an ability object (CR 112.1, 113.1).
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn arcane_stack(rule: Value) -> StackObject {
    StackObject {
        id: "stack:arcane-denial".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "arcane-denial",
            test_definition("arcane-denial", "Instant"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: Some(rule),
        decisions: BTreeMap::new(),
        targets: BTreeMap::from([(
            "targetSpell".to_string(),
            TargetRef::StackObject {
                stack_id: "stack:target-spell".to_string(),
            },
        )]),
    }
}

fn library_card(id: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, "Instant"), owner)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn captured_target_controller_chooses_two_and_caster_draws_one_at_next_upkeep() {
    let rules = parsed_rules();
    let mut engine = test_engine(2);
    // Only the delayed draws may put cards into these hands.
    engine.state.players[0].hand.clear();
    engine.state.players[1].hand.clear();
    engine.state.players[0].library = vec![library_card("caster-draw", "player-0")];
    engine.state.players[1].library = vec![
        library_card("target-draw-one", "player-1"),
        library_card("target-draw-two", "player-1"),
    ];
    engine.state.stack.push(stack_spell(false));
    let arcane = arcane_stack(rules[0].clone());

    execute_effects_at_resolution_start(
        &mut engine,
        &rules[0]["effects"],
        &arcane,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut MaximumDrawProvider,
    )
    .expect("counter and first delayed draw install");
    execute_effects_at_resolution_start(
        &mut engine,
        &rules[1]["effects"],
        &arcane,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut MaximumDrawProvider,
    )
    .expect("caster delayed draw installs");

    assert!(engine.state.stack.is_empty());
    assert_eq!(engine.state.players[0].hand.len(), 0);
    assert_eq!(engine.state.players[1].hand.len(), 0);
    assert_eq!(
        engine
            .state
            .rule_modifiers
            .iter()
            .filter(|modifier| value_kind(modifier) == Some("delayedStepTrigger"))
            .count(),
        2
    );

    engine.enqueue_step_triggers("upkeep");
    assert_eq!(engine.state.stack.len(), 2);
    engine
        .resolve_top_stack(&mut MaximumDrawProvider)
        .expect("caster delayed draw resolves");
    engine
        .resolve_top_stack(&mut MaximumDrawProvider)
        .expect("target-controller optional delayed draw resolves");

    assert_eq!(engine.state.players[0].hand.len(), 1);
    assert_eq!(engine.state.players[1].hand.len(), 2);
    assert!(
        engine
            .state
            .rule_modifiers
            .iter()
            .all(|modifier| value_kind(modifier) != Some("delayedStepTrigger"))
    );
    engine.enqueue_step_triggers("upkeep");
    assert!(
        engine.state.stack.is_empty(),
        "delayed triggers fire only once"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn uncounterable_target_remains_but_its_controller_still_gets_the_delayed_draw() {
    let rules = parsed_rules();
    let mut engine = test_engine(2);
    // Only the delayed draws may put cards into these hands.
    engine.state.players[0].hand.clear();
    engine.state.players[1].hand.clear();
    engine.state.players[1].library = vec![
        library_card("uncounterable-draw-one", "player-1"),
        library_card("uncounterable-draw-two", "player-1"),
    ];
    engine.state.stack.push(stack_spell(true));
    let arcane = arcane_stack(rules[0].clone());

    execute_effects_at_resolution_start(
        &mut engine,
        &rules[0]["effects"],
        &arcane,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut MaximumDrawProvider,
    )
    .expect("failed counter does not abort the following delayed effect");

    assert!(
        engine
            .state
            .stack
            .iter()
            .any(|object| object.id == "stack:target-spell")
    );
    assert_eq!(engine.state.players[1].hand.len(), 0);
    engine.enqueue_step_triggers("upkeep");
    assert_eq!(
        engine.state.stack.len(),
        2,
        "target spell plus delayed trigger"
    );
    engine
        .resolve_top_stack(&mut MaximumDrawProvider)
        .expect("uncounterable target controller delayed draw resolves");
    assert_eq!(engine.state.players[1].hand.len(), 2);
    assert!(
        engine
            .state
            .stack
            .iter()
            .any(|object| object.id == "stack:target-spell")
    );
}
