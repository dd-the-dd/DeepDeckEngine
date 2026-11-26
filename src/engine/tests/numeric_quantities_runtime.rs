//! Runtime proofs of the shared numeric quantity slots on the IR the Oracle
//! compiler emits: the universal determiner of a counter cost (CR 122.8),
//! a `for each` count scaling a catalog token creation (CR 111.10), and a
//! choice among counter kinds an entering permanent receives (CR 614.1c).
use super::*;

/// Picks the replacement option whose label is listed next, in order.
struct LabelProvider {
    labels: Vec<&'static str>,
}

impl DecisionProvider for LabelProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        let wanted = self.labels.remove(0);
        request
            .options
            .iter()
            .position(|option| option.label == wanted)
            .ok_or_else(|| EngineError::new(format!("no option labeled `{wanted}`")))
    }
}

/// A program that offers no choice: any request is a failure.
struct NoChoice;

impl DecisionProvider for NoChoice {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Err(EngineError::new(format!(
            "no decision expected, got {:?}",
            request.kind
        )))
    }
}

/// The canonical, executable rules of one Oracle text.
fn rules(name: &str, type_line: &str, mana_cost: &str, oracle_text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    parsed
        .abilities
        .into_iter()
        .map(|ability| {
            let rule = ability.rule.expect("canonical rule");
            assert!(rule_is_executable(&rule), "{rule:#}");
            rule
        })
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn removing_all_counters_declares_exactly_the_sources_count() {
    const TEXT: &str = "{T}, Remove all charge counters from this artifact: You gain 2 life for each charge counter removed this way.";
    let rule = rules("Bottle Probe", "Artifact", "{2}", TEXT).remove(0);
    for counters in [0, 3] {
        let mut engine = test_engine(2);
        let mut definition = test_definition("bottle-probe", "Artifact");
        definition.rules = vec![rule.clone()];
        let mut instance = test_instance("bottle-probe", definition, "player-0");
        instance.counters.insert("charge".to_string(), counters);
        engine.state.players[0].battlefield = vec![instance];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture admission");
        // `all`: the number is fixed by the source's charge counters when
        // the activation is declared; no smaller number is offered.
        let mut offered = engine
            .declarations_for_activated_rule(&rule, "player-0", "bottle-probe")
            .expect("declarations")
            .into_iter()
            .filter_map(|declaration| declaration.decisions.get("costChosenAmount").cloned())
            .filter_map(|value| value.as_i64())
            .collect::<Vec<_>>();
        offered.sort_unstable();
        offered.dedup();
        assert_eq!(offered, vec![i64::from(counters)], "{counters} counters");
    }
}

fn stack_object(card: CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:numeric-quantities".to_string(),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: rule["kind"].as_str().map(str::to_string),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

fn tokens_named(engine: &GameEngine, name: &str) -> usize {
    engine.state.players[0]
        .battlefield
        .iter()
        .filter(|permanent| permanent.definition.is_token && permanent.definition.name == name)
        .count()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_for_each_count_scales_a_catalog_token_creation() {
    // One Food per creature, and a literal two per artifact: the same
    // predefined identity with the scaled quantity.
    for (text, name, expected) in [
        (
            "Create a Food token for each creature you control.",
            "Food",
            3,
        ),
        (
            "Create two Clue tokens for each artifact you control.",
            "Clue",
            4,
        ),
    ] {
        let rule = rules("Token Probe", "Sorcery", "{2}{G}", text).remove(0);
        let mut engine = test_engine(2);
        engine.state.players[0].battlefield = (0..3)
            .map(|index| {
                test_instance(
                    &format!("bear-{index}"),
                    test_definition(&format!("bear-{index}"), "Creature - Bear"),
                    "player-0",
                )
            })
            .chain((0..2).map(|index| {
                test_instance(
                    &format!("relic-{index}"),
                    test_definition(&format!("relic-{index}"), "Artifact"),
                    "player-0",
                )
            }))
            .collect();
        let spell = test_instance(
            "token-probe",
            test_definition("token-probe", "Sorcery"),
            "player-0",
        );
        execute_effects_at_resolution_start(
            &mut engine,
            &rule["effects"],
            &stack_object(spell, &rule),
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut NoChoice,
        )
        .expect("the compiled program executes");
        assert_eq!(tokens_named(&engine, name), expected, "{text}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_entering_permanent_receives_only_the_chosen_counter_kind() {
    const TEXT: &str =
        "This creature enters with your choice of a trample counter or a menace counter on it.";
    let rule = rules("Choice Probe", "Creature — Cat Ape", "{2}{G}", TEXT).remove(0);
    for (label, chosen, other) in [
        ("Choose menace", "menace", "trample"),
        ("Choose trample", "trample", "menace"),
    ] {
        let mut engine = test_engine(2);
        let mut definition = test_definition("choice-probe", "Creature - Cat Ape");
        definition.rules = vec![rule.clone()];
        let mut source = test_instance("choice-probe", definition, "player-0");
        engine
            .apply_enter_replacements(
                &mut source,
                &mut LabelProvider {
                    labels: vec![label],
                },
            )
            .expect("the entry choice applies");
        assert_eq!(source.counters.get(chosen), Some(&1), "{label}");
        assert_eq!(
            source.counters.get(other).copied().unwrap_or(0),
            0,
            "{label}"
        );
    }
}
