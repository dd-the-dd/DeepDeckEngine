use super::*;

// CR 118.3 / 701.26a: tapping untapped permanents the payer controls as a
// resolution cost. `other`/`another` excludes the source object.
const TWO_OTHERS: &str = "Whenever this creature attacks, you may tap two other untapped creatures you control. If you do, draw a card.";
const ONE_ANOTHER: &str = "Whenever this creature attacks, you may tap another untapped creature you control. If you do, draw a card.";

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

fn parsed_effect(text: &str) -> (Value, Value) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Tap Source".to_string(),
        type_line: "Creature - Human".to_string(),
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

fn admitted(engine: &mut GameEngine, id: &str) -> CardInstance {
    let mut card = test_instance(id, test_definition(id, "Creature"), "player-0");
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn resolve(engine: &mut GameEngine, source: CardInstance, rule: &Value, effect: &Value) {
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
            &mut Pay,
        )
        .expect("the optional cost program resolves");
}

fn tapped_ids(engine: &GameEngine) -> Vec<String> {
    engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.tapped)
        .map(|permanent| permanent.instance_id.clone())
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn tap_many_cost_excludes_the_source_and_needs_enough_untapped_creatures() {
    let (rule, effect) = parsed_effect(TWO_OTHERS);
    assert_eq!(effect["cost"]["kind"], "tapPermanents");
    assert_eq!(effect["cost"]["excludeSource"], true);

    let mut engine = test_engine(2);
    let source = admitted(&mut engine, "source");
    let first = admitted(&mut engine, "first");
    engine.state.players[0].battlefield = vec![source.clone(), first];
    let hand = engine.state.players[0].hand.len();
    // The source cannot pay: one other creature is not enough for two.
    resolve(&mut engine, source.clone(), &rule, &effect);
    assert!(tapped_ids(&engine).is_empty());
    assert_eq!(engine.state.players[0].hand.len(), hand);

    let second = admitted(&mut engine, "second");
    engine.state.players[0].battlefield.push(second);
    resolve(&mut engine, source, &rule, &effect);
    let tapped = tapped_ids(&engine);
    assert_eq!(tapped.len(), 2);
    assert!(!tapped.contains(&"source".to_string()));
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn tap_one_cost_excludes_the_source() {
    let (rule, effect) = parsed_effect(ONE_ANOTHER);
    assert_eq!(effect["cost"]["kind"], "tap");
    assert_eq!(effect["cost"]["excludeSource"], true);
    let mut engine = test_engine(2);
    let source = admitted(&mut engine, "source");
    engine.state.players[0].battlefield = vec![source.clone()];
    let hand = engine.state.players[0].hand.len();
    resolve(&mut engine, source.clone(), &rule, &effect);
    assert!(
        tapped_ids(&engine).is_empty(),
        "only the source is untapped"
    );
    let other = admitted(&mut engine, "other");
    engine.state.players[0].battlefield.push(other);
    resolve(&mut engine, source, &rule, &effect);
    assert_eq!(tapped_ids(&engine), vec!["other".to_string()]);
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
}
