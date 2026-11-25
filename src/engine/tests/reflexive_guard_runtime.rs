use super::*;

// `When you do, if CONDITION, BODY.` (Sigil of Myrkul): the intervening `if`
// of a reflexive ability (CR 603.4, 603.12). Oracle text -> canonical rule ->
// the reflexive ability is put on the stack only when the condition holds,
// and resolves only when it still holds.
const TEXT: &str = "When this creature enters, you may discard a card. When you do, if you control an artifact, draw a card.";

/// Pays every offered optional resolution cost; any other decision (the
/// discarded card) takes its first option.
struct Perform;

impl DecisionProvider for Perform {
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

fn parsed_rule() -> Value {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Reflexive Guard Probe".to_string(),
        type_line: "Creature — Human Cleric".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(TEXT.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    // An optional discard is an optional resolution cost (the established
    // batch-9 shape); its reflexive ability is the performed effect.
    assert_eq!(rule["effects"][0]["kind"], "optionalPayCostPerformEffects");
    let reflexive = &rule["effects"][0]["effects"][0];
    assert_eq!(reflexive["kind"], "createReflexiveTrigger", "{rule:#}");
    assert!(
        reflexive["ability"]["condition"].is_object(),
        "the intervening if conditions the reflexive ability: {reflexive:#}"
    );
    rule
}

fn card(engine: &mut GameEngine, id: &str, type_line: &str) -> CardInstance {
    let mut card = test_instance(id, test_definition(id, type_line), "player-0");
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn setup(with_artifact: bool) -> (GameEngine, CardInstance) {
    let mut engine = test_engine(2);
    let source = card(&mut engine, "source", "Creature — Human Cleric");
    let mut battlefield = vec![source.clone()];
    if with_artifact {
        battlefield.push(card(&mut engine, "relic", "Artifact"));
    }
    let hand = vec![card(&mut engine, "hand-card", "Instant")];
    let library = (0..3)
        .map(|index| card(&mut engine, &format!("library-{index}"), "Sorcery"))
        .collect::<Vec<_>>();
    engine.state.players[0].battlefield = battlefield;
    engine.state.players[0].hand = hand;
    engine.state.players[0].library = library;
    (engine, source)
}

fn resolve_trigger(engine: &mut GameEngine, source: &CardInstance, rule: &Value) {
    let stack_object = StackObject {
        id: "stack:entry-trigger".to_string(),
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
        .execute_effect(
            &rule["effects"][0],
            None,
            &stack_object,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut Perform,
        )
        .expect("the producing instruction resolves");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_false_intervening_if_creates_no_reflexive_ability() {
    let rule = parsed_rule();
    let (mut engine, source) = setup(false);
    let stack = engine.state.stack.len();
    resolve_trigger(&mut engine, &source, &rule);
    assert_eq!(engine.state.players[0].graveyard.len(), 1, "the discard");
    assert_eq!(engine.state.stack.len(), stack, "no reflexive ability");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_intervening_if_is_checked_again_as_the_reflexive_ability_resolves() {
    let rule = parsed_rule();
    // True when it triggers and when it resolves: the card is drawn.
    let (mut engine, source) = setup(true);
    resolve_trigger(&mut engine, &source, &rule);
    assert_eq!(engine.state.stack.len(), 1, "the reflexive ability waits");
    engine
        .resolve_top_stack(&mut Perform)
        .expect("the reflexive ability resolves");
    assert_eq!(engine.state.players[0].library.len(), 2, "drawn");
    // True when it triggers, false when it resolves: nothing happens.
    let (mut engine, source) = setup(true);
    resolve_trigger(&mut engine, &source, &rule);
    assert_eq!(engine.state.stack.len(), 1, "the reflexive ability waits");
    engine.state.players[0]
        .battlefield
        .retain(|permanent| permanent.definition.name != "relic");
    engine
        .resolve_top_stack(&mut Perform)
        .expect("the reflexive ability resolves");
    assert_eq!(engine.state.players[0].library.len(), 3, "not drawn");
}
