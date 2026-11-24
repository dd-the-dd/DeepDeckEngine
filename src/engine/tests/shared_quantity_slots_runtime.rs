//! Shared quantity slots, Oracle text -> IR -> engine: an event amount
//! (`that many` after a life gain), half of a player fact with its stated
//! rounding, `remove all KIND counters` as the recipient's own count, and a
//! discard whose amount the discarding player chooses (`any number of`).
use super::*;

fn parse_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn stack_object(
    source: &CardInstance,
    rule: &Value,
    targets: BTreeMap<String, TargetRef>,
) -> StackObject {
    StackObject {
        id: "stack:shared-quantity".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

fn execute(
    engine: &mut GameEngine,
    source: &CardInstance,
    effects: Value,
    decisions: &mut BTreeMap<String, Value>,
    targets: BTreeMap<String, TargetRef>,
) {
    let rule = json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
        "effects": effects,
    });
    assert!(rule_is_executable(&rule), "{rule:#}");
    let object = stack_object(source, &rule, targets);
    engine
        .execute_effects(
            &rule["effects"],
            &object,
            &mut BTreeMap::new(),
            decisions,
            &mut EmeritusDecisionProvider,
        )
        .expect("the effects resolve");
}

/// Ageless Entity: the life-gain occurrence records its amount, and `that
/// many` reads it, so gaining 3 life puts three counters (CR 119.10).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn gaining_three_life_puts_that_many_counters() {
    let mut definition = test_definition("entity", "Creature — Elemental");
    definition.power = Some("4".to_string());
    definition.toughness = Some("4".to_string());
    definition.rules = parse_rules(
        "Ageless Entity",
        "Creature — Elemental",
        "Whenever you gain life, put that many +1/+1 counters on this creature.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![test_instance("entity", definition, "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine.gain_life("player-0", 3).expect("life is gained");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the trigger resolves");
    assert_eq!(
        engine.state.players[0].battlefield[0]
            .counters
            .get("+1/+1")
            .copied()
            .unwrap_or_default(),
        3
    );
}

/// Cruel Bargain: half of 7 rounded up is 4, so the player goes to 3.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn half_of_an_odd_life_total_rounds_up_as_written() {
    let rules = parse_rules(
        "Half Life Probe",
        "Sorcery",
        "You lose half your life, rounded up.",
    );
    let source = test_instance("bargain", test_definition("bargain", "Sorcery"), "player-0");
    let mut engine = test_engine(2);
    engine.state.players[0].life = 7;
    execute(
        &mut engine,
        &source,
        rules[0]["effects"].clone(),
        &mut BTreeMap::new(),
        BTreeMap::new(),
    );
    assert_eq!(engine.state.players[0].life, 3);
}

/// `remove all KIND counters from OBJECT` removes that object's own count of
/// the kind, on the source and on a declared target alike; other kinds stay.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn removing_all_counters_of_a_kind_reads_the_recipient_count() {
    let mut source = test_instance(
        "crocodile",
        test_definition("crocodile", "Creature — Zombie Crocodile Demon"),
        "player-0",
    );
    source.counters.insert("-1/-1".to_string(), 3);
    source.counters.insert("+1/+1".to_string(), 1);
    let mut other = test_instance(
        "other",
        test_definition("other", "Creature — Bear"),
        "player-1",
    );
    other.counters.insert("-1/-1".to_string(), 2);
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![source.clone()];
    engine.state.players[1].battlefield = vec![other];
    let removal = |object: Value| {
        json!({
            "kind": "removeCounters",
            "permanent": object.clone(),
            "counter": "-1/-1",
            "count": { "kind": "counterCountOf", "object": object, "counter": "-1/-1" },
        })
    };
    let target = json!({ "kind": "chosenTarget", "id": "targetPermanent" });
    execute(
        &mut engine,
        &source,
        json!([removal(json!({ "kind": "self" })), removal(target)]),
        &mut BTreeMap::new(),
        BTreeMap::from([(
            "targetPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: "other".to_string(),
            },
        )]),
    );
    let counters = |player: usize| engine.state.players[player].battlefield[0].counters.clone();
    assert_eq!(counters(0).get("-1/-1").copied().unwrap_or_default(), 0);
    assert_eq!(counters(0).get("+1/+1").copied().unwrap_or_default(), 1);
    assert_eq!(counters(1).get("-1/-1").copied().unwrap_or_default(), 0);
    // A literal count composes with the same object slot (Clockwork Beetle's
    // delayed `remove a +1/+1 counter from it` on a captured object).
    engine.state.players[1].battlefield[0]
        .counters
        .insert("+1/+1".to_string(), 2);
    execute(
        &mut engine,
        &source,
        json!([{
            "kind": "removeCounters",
            "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
            "counter": "+1/+1",
            "count": 1,
        }]),
        &mut BTreeMap::new(),
        BTreeMap::from([(
            "targetPermanent".to_string(),
            TargetRef::Permanent {
                instance_id: "other".to_string(),
            },
        )]),
    );
    assert_eq!(
        engine.state.players[1].battlefield[0].counters.get("+1/+1"),
        Some(&1)
    );
}

/// Rites of Spring: `Discard any number of cards` is a choice of zero to
/// every card in hand, then a discard of exactly the chosen cards.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn any_number_discard_discards_exactly_the_chosen_cards() {
    let rules = parse_rules("Discard Probe", "Sorcery", "Discard any number of cards.");
    let source = test_instance("rites", test_definition("rites", "Sorcery"), "player-0");
    let mut engine = test_engine(2);
    engine.state.players[0].hand = (0..3)
        .map(|index| {
            let id = format!("card-{index}");
            test_instance(&id, test_definition(&id, "Instant"), "player-0")
        })
        .collect();
    // The chosen cards are bound by their admitted incarnations.
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let mut decisions = BTreeMap::new();
    execute(
        &mut engine,
        &source,
        rules[0]["effects"].clone(),
        &mut decisions,
        BTreeMap::new(),
    );
    let chosen = decisions
        .values()
        .find_map(Value::as_array)
        .map_or(0, Vec::len);
    assert!(chosen <= 3, "at most every card in hand");
    assert_eq!(engine.state.players[0].hand.len(), 3 - chosen);
    assert_eq!(engine.state.players[0].graveyard.len(), chosen);
}
