use super::*;

// Real Oracle texts: Automated Warfare System (sacrifice another … If you do)
// and Tether Technician (discard … When you do, reflexive target).
const SACRIFICE_TEXT: &str = "Whenever this creature attacks, you may sacrifice another creature or artifact. If you do, draw a card.";
const REFLEXIVE_TEXT: &str = "When this creature enters, you may discard a card. When you do, this creature deals 2 damage to any target.";

/// Chooses to pay the offered resolution cost (or reflexive targets).
struct Pay;
impl DecisionProvider for Pay {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .position(|option| option.kind == ActionKind::ChooseResolution)
            .unwrap_or(0))
    }
}

/// Declines every optional resolution cost.
struct Decline;
impl DecisionProvider for Decline {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(request
            .options
            .iter()
            .position(|option| option.kind != ActionKind::ChooseResolution)
            .unwrap_or(0))
    }
}

fn parsed_effect(name: &str, text: &str) -> (Value, Value) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Artifact Creature — Robot".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    let effect = rule["effects"][0].clone();
    assert_eq!(effect["kind"], "optionalPayCostPerformEffects");
    (rule, effect)
}

fn admitted(engine: &mut GameEngine, mut card: CardInstance) -> CardInstance {
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn resolve(
    engine: &mut GameEngine,
    source: CardInstance,
    rule: &Value,
    effect: &Value,
    provider: &mut impl DecisionProvider,
) {
    let stack_object = StackObject {
        id: "stack:trigger".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .execute_effect(
            effect,
            None,
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            provider,
        )
        .expect("the optional cost program resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn if_you_do_runs_only_after_sacrificing_another_permanent() {
    let (rule, effect) = parsed_effect("Automated Warfare System", SACRIFICE_TEXT);
    assert_eq!(effect["cost"]["excludeSource"], true);

    // Only the source is on the battlefield: `another` makes the cost unpayable.
    let mut engine = test_engine(2);
    let source = admitted(
        &mut engine,
        test_instance(
            "source",
            test_definition("source", "Artifact Creature"),
            "player-0",
        ),
    );
    engine.state.players[0].battlefield = vec![source.clone()];
    let hand = engine.state.players[0].hand.len();
    resolve(&mut engine, source.clone(), &rule, &effect, &mut Pay);
    assert_eq!(
        engine.state.players[0].battlefield.len(),
        1,
        "the source is never sacrificed"
    );
    assert_eq!(
        engine.state.players[0].hand.len(),
        hand,
        "no draw without payment"
    );

    // Another creature: paying sacrifices it, keeps the source, and draws.
    let fodder = admitted(
        &mut engine,
        test_instance("fodder", test_definition("fodder", "Creature"), "player-0"),
    );
    engine.state.players[0].battlefield.push(fodder);
    resolve(&mut engine, source.clone(), &rule, &effect, &mut Pay);
    let battlefield = &engine.state.players[0].battlefield;
    assert_eq!(battlefield.len(), 1);
    assert_eq!(battlefield[0].instance_id, "source");
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);

    // Declining performs nothing.
    let fodder = admitted(
        &mut engine,
        test_instance(
            "fodder-2",
            test_definition("fodder-2", "Artifact"),
            "player-0",
        ),
    );
    engine.state.players[0].battlefield.push(fodder);
    resolve(&mut engine, source, &rule, &effect, &mut Decline);
    assert_eq!(engine.state.players[0].battlefield.len(), 2);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn when_you_do_puts_a_reflexive_trigger_with_its_own_target_on_the_stack() {
    let (rule, effect) = parsed_effect("Tether Technician", REFLEXIVE_TEXT);
    assert!(
        rule["declaration"].is_null(),
        "the outer trigger declares no target"
    );
    let mut engine = test_engine(2);
    let source = admitted(
        &mut engine,
        test_instance(
            "source",
            test_definition("source", "Artifact Creature"),
            "player-0",
        ),
    );
    engine.state.players[0].battlefield = vec![source.clone()];
    let card = admitted(
        &mut engine,
        test_instance(
            "discarded",
            test_definition("discarded", "Instant"),
            "player-0",
        ),
    );
    engine.state.players[0].hand.push(card);

    let stack = engine.state.stack.len();
    resolve(&mut engine, source.clone(), &rule, &effect, &mut Decline);
    assert_eq!(
        engine.state.stack.len(),
        stack,
        "declining creates no reflexive trigger"
    );

    let hand = engine.state.players[0].hand.len();
    resolve(&mut engine, source, &rule, &effect, &mut Pay);
    assert_eq!(
        engine.state.players[0].hand.len(),
        hand - 1,
        "a card was discarded"
    );
    assert_eq!(engine.state.stack.len(), stack + 1);
    let reflexive = engine.state.stack.last().expect("reflexive trigger");
    assert_eq!(
        reflexive.targets.len(),
        1,
        "targets are chosen as it is put on the stack"
    );
    assert_eq!(
        reflexive.ability_rule.as_ref().expect("reflexive rule")["effects"][0]["kind"],
        "dealDamage"
    );
}
