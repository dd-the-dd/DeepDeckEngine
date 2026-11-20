//! Real Oracle text -> primitive IR -> engine behavior for the shared object
//! query predicates: untapped status (CR 110.5), counter qualifiers with and
//! without (CR 122.1), the snow supertype (CR 205.4a), permanent cards
//! (CR 110.4b) with a numeric search qualifier, and `other` source exclusion.
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
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

fn permanent(id: &str, type_line: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), owner)
}

fn card_with_cost(id: &str, type_line: &str, mana_cost: &str, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    test_instance(id, definition, owner)
}

fn resolve(engine: &mut GameEngine, rule: &Value, card: CardInstance) {
    let stack = StackObject {
        id: format!("stack:{}", card.instance_id),
        controller: "player-0".to_string(),
        card,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        &stack,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the parsed program resolves");
}

fn battlefield_ids(engine: &GameEngine, player: usize) -> Vec<String> {
    engine.state.players[player]
        .battlefield
        .iter()
        .map(|card| card.instance_id.clone())
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn untapped_status_and_snow_supertype_filter_a_destroyed_collection() {
    let untapped = parsed_rules(
        "Untapped Sweep",
        "Sorcery",
        "Destroy all untapped creatures.",
    );
    let mut engine = test_engine(2);
    let mut tapped = permanent("tapped-bear", "Creature - Bear", "player-1");
    tapped.tapped = true;
    engine.state.players[1].battlefield = vec![
        tapped,
        permanent("untapped-bear", "Creature - Bear", "player-1"),
        permanent("untapped-land", "Land", "player-1"),
    ];
    resolve(
        &mut engine,
        &untapped[0],
        permanent("sweep", "Sorcery", "player-0"),
    );
    assert_eq!(
        battlefield_ids(&engine, 1),
        ["tapped-bear", "untapped-land"],
        "only untapped creatures are destroyed"
    );

    let snow = parsed_rules("Snow Sweep", "Sorcery", "Destroy all snow creatures.");
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = vec![
        permanent("snow-bear", "Snow Creature - Bear", "player-1"),
        permanent("plain-bear", "Creature - Bear", "player-1"),
        permanent("snow-land", "Snow Land", "player-1"),
    ];
    resolve(
        &mut engine,
        &snow[0],
        permanent("snow-sweep", "Sorcery", "player-0"),
    );
    assert_eq!(battlefield_ids(&engine, 1), ["plain-bear", "snow-land"]);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn counter_qualifiers_read_live_counters_with_and_without() {
    let with_kind = parsed_rules(
        "Counter Sweep",
        "Sorcery",
        "Destroy each creature with a +1/+1 counter on it.",
    );
    let without_any = parsed_rules(
        "Clean Sweep",
        "Sorcery",
        "Destroy each creature without a counter on it.",
    );
    let board = || {
        let mut plus = permanent("plus-bear", "Creature - Bear", "player-1");
        plus.counters.insert("+1/+1".to_string(), 1);
        let mut minus = permanent("minus-bear", "Creature - Bear", "player-1");
        minus.counters.insert("-1/-1".to_string(), 1);
        vec![
            plus,
            minus,
            permanent("bare-bear", "Creature - Bear", "player-1"),
        ]
    };
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = board();
    resolve(
        &mut engine,
        &with_kind[0],
        permanent("s1", "Sorcery", "player-0"),
    );
    assert_eq!(
        battlefield_ids(&engine, 1),
        ["minus-bear", "bare-bear"],
        "only the named counter kind matches"
    );

    let mut engine = test_engine(2);
    engine.state.players[1].battlefield = board();
    resolve(
        &mut engine,
        &without_any[0],
        permanent("s2", "Sorcery", "player-0"),
    );
    assert_eq!(
        battlefield_ids(&engine, 1),
        ["plus-bear", "minus-bear"],
        "`without a counter` keeps creatures with a counter of any kind"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn other_excludes_only_the_ability_source() {
    let rules = parsed_rules(
        "Jealous Titan",
        "Creature - Giant",
        "{1}: Destroy all other creatures.",
    );
    let source = permanent("titan", "Creature - Giant", "player-0");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![
        source.clone(),
        permanent("friendly-giant", "Creature - Giant", "player-0"),
    ];
    engine.state.players[1].battlefield =
        vec![permanent("enemy-bear", "Creature - Bear", "player-1")];
    resolve(&mut engine, &rules[0], source);
    assert_eq!(battlefield_ids(&engine, 0), ["titan"]);
    assert!(battlefield_ids(&engine, 1).is_empty());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn permanent_card_search_shares_the_numeric_qualifier() {
    let rules = parsed_rules(
        "Permanent Tutor",
        "Sorcery",
        "Search your library for a permanent card with mana value 3 or less, reveal it, put it into your hand, then shuffle.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = vec![
        card_with_cost("cheap-instant", "Instant", "{1}", "player-0"),
        card_with_cost("big-creature", "Creature - Giant", "{5}", "player-0"),
        card_with_cost("cheap-artifact", "Artifact", "{2}", "player-0"),
    ];
    resolve(
        &mut engine,
        &rules[0],
        permanent("tutor", "Sorcery", "player-0"),
    );
    let hand = engine.state.players[0]
        .hand
        .iter()
        .map(|card| card.instance_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        hand,
        ["cheap-artifact"],
        "an instant is not a permanent card and the creature is above the threshold"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn counter_qualified_target_candidates_read_the_permanents_live_counters() {
    let rules = parsed_rules(
        "Counter Hunter",
        "Instant",
        "Destroy target creature with a +1/+1 counter on it.",
    );
    let candidates = &rules[0]["declaration"]["decisions"][0]["candidates"];
    let mut engine = test_engine(2);
    let mut plus = permanent("plus-bear", "Creature - Bear", "player-1");
    plus.counters.insert("+1/+1".to_string(), 2);
    let mut minus = permanent("minus-bear", "Creature - Bear", "player-1");
    minus.counters.insert("-1/-1".to_string(), 1);
    engine.state.players[1].battlefield = vec![plus, minus];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let legal = engine
        .target_candidates("player-0", candidates)
        .expect("the candidate query is evaluable");
    let rendered = format!("{legal:?}");
    assert_eq!(legal.len(), 1, "{rendered}");
    assert!(rendered.contains("plus-bear"), "{rendered}");
}
