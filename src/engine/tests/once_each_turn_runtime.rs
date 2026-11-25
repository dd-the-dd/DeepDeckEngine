use super::*;

// `Do this only once each turn.` (Ondu Spiritdancer, Irreverent Gremlin): the
// rider limits performing the ability's optional instruction, never the
// ability. Oracle text -> canonical rule -> resolution of the same rule of the
// same object twice in one turn, then on a later turn.
const LOOT_TEXT: &str = "Whenever another creature you control enters, you may discard a card. If you do, draw a card. Do this only once each turn.";
const LIFE_TEXT: &str = "Whenever another creature you control enters, you may gain 2 life. Do this only once each turn.";

/// Accepts (`self.perform`) or declines every optional instruction; any
/// other decision (the discarded card) takes its first option.
struct Answer {
    perform: bool,
}

impl DecisionProvider for Answer {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let perform = self.perform;
        Ok(request
            .options
            .iter()
            .position(
                |option| match option.decisions.get("performOptionalAction") {
                    Some(chosen) => *chosen == Value::Bool(perform),
                    None => {
                        let declines = option.kind == ActionKind::DeclinePayment
                            || option.label.starts_with("Decline");
                        declines != perform
                    }
                },
            )
            .unwrap_or(0))
    }
}

fn parsed_effect(text: &str) -> (Value, Value) {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Once Each Turn Probe".to_string(),
        type_line: "Creature — Human Advisor".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("canonical rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    let effect = rule["effects"][0].clone();
    assert_eq!(
        effect["oncePerTurn"], "doThisOnlyOnceEachTurn",
        "the rider marks the one optional instruction: {effect:#}"
    );
    (rule, effect)
}

fn card(engine: &mut GameEngine, id: &str, type_line: &str) -> CardInstance {
    let mut card = test_instance(id, test_definition(id, type_line), "player-0");
    engine
        .assign_new_object_incarnation(&mut card)
        .expect("admit test object");
    card
}

fn resolve(engine: &mut GameEngine, source: &CardInstance, rule: &Value, perform: bool) {
    let stack_object = StackObject {
        id: format!("stack:trigger:{}", engine.state.events.len()),
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
            &mut Answer { perform },
        )
        .expect("the limited optional instruction resolves");
}

fn loot_engine() -> (GameEngine, CardInstance) {
    let mut engine = test_engine(2);
    let source = card(&mut engine, "source", "Creature — Gremlin");
    engine.state.players[0].battlefield = vec![source.clone()];
    let hand = (0..3)
        .map(|index| card(&mut engine, &format!("hand-{index}"), "Instant"))
        .collect::<Vec<_>>();
    let library = (0..3)
        .map(|index| card(&mut engine, &format!("library-{index}"), "Sorcery"))
        .collect::<Vec<_>>();
    engine.state.players[0].hand = hand;
    engine.state.players[0].library = library;
    engine.state.players[0].graveyard.clear();
    (engine, source)
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_performed_instruction_is_spent_until_the_next_turn() {
    let (rule, _) = parsed_effect(LOOT_TEXT);
    let (mut engine, source) = loot_engine();
    resolve(&mut engine, &source, &rule, true);
    assert_eq!(engine.state.players[0].graveyard.len(), 1, "first loot");
    assert_eq!(engine.state.players[0].library.len(), 2, "first draw");
    // The ability resolves again this turn: the instruction is not offered.
    resolve(&mut engine, &source, &rule, true);
    assert_eq!(
        engine.state.players[0].graveyard.len(),
        1,
        "spent this turn"
    );
    assert_eq!(engine.state.players[0].library.len(), 2, "no second draw");
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "optionalActionUnavailable"),
        "the spent instruction is unavailable, not silently skipped"
    );
    // A later turn offers it again.
    engine.state.turn_number += 1;
    resolve(&mut engine, &source, &rule, true);
    assert_eq!(engine.state.players[0].graveyard.len(), 2, "next turn loot");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_declined_instruction_leaves_the_limit_unspent() {
    let (rule, _) = parsed_effect(LOOT_TEXT);
    let (mut engine, source) = loot_engine();
    resolve(&mut engine, &source, &rule, false);
    assert_eq!(engine.state.players[0].graveyard.len(), 0, "declined");
    resolve(&mut engine, &source, &rule, true);
    assert_eq!(
        engine.state.players[0].graveyard.len(),
        1,
        "still available"
    );
    // Another object with the same ability has its own limit (CR 400.7).
    let other = card(&mut engine, "other-source", "Creature — Gremlin");
    engine.state.players[0].battlefield.push(other.clone());
    resolve(&mut engine, &other, &rule, true);
    assert_eq!(engine.state.players[0].graveyard.len(), 2, "own limit");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_unmeasured_optional_instruction_is_limited_the_same_way() {
    let (rule, _) = parsed_effect(LIFE_TEXT);
    let mut engine = test_engine(2);
    let source = card(&mut engine, "source", "Creature — Kor Cleric");
    engine.state.players[0].battlefield = vec![source.clone()];
    let life = engine.state.players[0].life;
    resolve(&mut engine, &source, &rule, true);
    resolve(&mut engine, &source, &rule, true);
    assert_eq!(engine.state.players[0].life, life + 2, "one gain this turn");
    engine.state.turn_number += 1;
    resolve(&mut engine, &source, &rule, true);
    assert_eq!(engine.state.players[0].life, life + 4, "one more next turn");
}
