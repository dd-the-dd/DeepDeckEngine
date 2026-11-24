//! Real Oracle text -> primitive IR -> engine behavior for a library search
//! that ends at its selection: the found cards are an ordinary collection the
//! program's later actions move, and the searched library is shuffled only by
//! a written instruction, possibly another player's (CR 701.23a, 608.2c). A
//! `look` followed by `put them back in any order` returns the looked-at cards
//! to the top of their library.
use super::*;

fn parsed_rule(type_line: &str, oracle: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: "Library Search Probe".to_string(),
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
    assert_eq!(parsed.status, "canonical", "{oracle}");
    let rule = parsed
        .abilities
        .into_iter()
        .find_map(|ability| ability.rule)
        .expect("one rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn spell(rule: &Value, targets: BTreeMap<String, TargetRef>) -> StackObject {
    StackObject {
        id: "stack:search-probe".to_string(),
        controller: "player-0".to_string(),
        card: test_instance(
            "search-probe",
            test_definition("search-probe", "Sorcery"),
            "player-0",
        ),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

fn declared_player_target(rule: &Value, player_id: &str) -> BTreeMap<String, TargetRef> {
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared player")
        .to_string();
    BTreeMap::from([(
        target_id,
        TargetRef::Player {
            player_id: player_id.to_string(),
        },
    )])
}

fn card(instance_id: &str, type_line: &str, owner: &str) -> CardInstance {
    test_instance(instance_id, test_definition(instance_id, type_line), owner)
}

fn ids(cards: &[CardInstance]) -> Vec<String> {
    cards.iter().map(|card| card.instance_id.clone()).collect()
}

fn resolve(engine: &mut GameEngine, rule: &Value, targets: BTreeMap<String, TargetRef>) {
    execute_effects_at_resolution_start(
        engine,
        &rule["effects"],
        &spell(rule, targets),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut EmeritusDecisionProvider,
    )
    .expect("the spell resolves");
}

fn shuffled(engine: &GameEngine, player_id: &str) -> bool {
    engine
        .state
        .events
        .iter()
        .any(|event| event.kind == "zoneShuffled" && event.player_id.as_deref() == Some(player_id))
}

/// Mana Severance: `any number of` has no maximum below the matching cards
/// of the searched library; every land may be found (the test provider takes
/// the largest selection) and nothing else is.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_unbounded_search_may_find_every_matching_card() {
    let rule = parsed_rule(
        "Sorcery",
        "Search your library for any number of land cards, exile them, then shuffle.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].library = vec![
        card("plains-a", "Basic Land - Plains", "player-0"),
        card("bear", "Creature - Bear", "player-0"),
        card("island-b", "Basic Land - Island", "player-0"),
        card("swamp-c", "Basic Land - Swamp", "player-0"),
    ];
    resolve(&mut engine, &rule, BTreeMap::new());
    let mut exiled = ids(&engine.state.players[0].exile);
    exiled.sort();
    assert_eq!(exiled, ["island-b", "plains-a", "swamp-c"]);
    assert_eq!(ids(&engine.state.players[0].library), ["bear"]);
    assert!(shuffled(&engine, "player-0"));
}

/// Bribery: the controller searches the declared opponent's library, the
/// found creature enters under the controller's control, and the shuffle is
/// the opponent's own later instruction (`Then that player shuffles.`).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_bound_search_result_moves_by_a_later_action_and_its_holder_shuffles() {
    let rule = parsed_rule(
        "Sorcery",
        "Search target opponent's library for a creature card and put that card onto the battlefield under your control. Then that player shuffles.",
    );
    let mut engine = test_engine(2);
    engine.state.players[1].library = vec![
        card("swamp", "Basic Land - Swamp", "player-1"),
        card("ogre", "Creature - Ogre", "player-1"),
    ];
    let controller_battlefield = engine.state.players[0].battlefield.len();
    resolve(
        &mut engine,
        &rule,
        declared_player_target(&rule, "player-1"),
    );
    let ogre = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "ogre")
        .expect("the found creature entered under the searcher's control");
    assert_eq!(ogre.owner, "player-1");
    assert_eq!(ogre.controller, "player-0");
    assert_eq!(
        engine.state.players[0].battlefield.len(),
        controller_battlefield + 1
    );
    assert_eq!(ids(&engine.state.players[1].library), ["swamp"]);
    assert!(shuffled(&engine, "player-1"), "the holder shuffles");
    assert!(
        !shuffled(&engine, "player-0"),
        "the searcher's own library is not shuffled"
    );
}

/// Final Parting: the bound cards are partitioned by the next sentence; one
/// chosen card goes to hand and the other to the graveyard, then the library
/// is shuffled.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn bound_found_cards_are_partitioned_by_later_sentences() {
    let rule = parsed_rule(
        "Sorcery",
        "Search your library for two cards. Put one into your hand and the other into your graveyard. Then shuffle.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].library = vec![
        card("first", "Artifact", "player-0"),
        card("second", "Enchantment", "player-0"),
        card("third", "Creature - Bear", "player-0"),
    ];
    let hand = engine.state.players[0].hand.len();
    let graveyard = engine.state.players[0].graveyard.len();
    resolve(&mut engine, &rule, BTreeMap::new());
    assert_eq!(engine.state.players[0].hand.len(), hand + 1);
    assert_eq!(engine.state.players[0].graveyard.len(), graveyard + 1);
    assert_eq!(engine.state.players[0].library.len(), 1);
    let kept = engine.state.players[0].library[0].instance_id.clone();
    assert!(
        !ids(&engine.state.players[0].hand).contains(&kept)
            && !ids(&engine.state.players[0].graveyard).contains(&kept),
        "the card left in the library was not found"
    );
    assert!(shuffled(&engine, "player-0"));
}

/// Index: `put them back in any order` returns exactly the looked-at cards to
/// the top of their library; the rest of the library is untouched.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn looked_at_cards_put_back_stay_on_top_of_their_library() {
    let rule = parsed_rule(
        "Sorcery",
        "Look at the top five cards of your library, then put them back in any order.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].library = (0..7)
        .map(|index| card(&format!("card-{index}"), "Artifact", "player-0"))
        .collect();
    let before = ids(&engine.state.players[0].library);
    resolve(&mut engine, &rule, BTreeMap::new());
    let after = ids(&engine.state.players[0].library);
    assert_eq!(after.len(), before.len());
    // The library top is the end of the vector: the two bottom cards stay.
    assert_eq!(after[..2], before[..2]);
    let mut top_before = before[2..].to_vec();
    let mut top_after = after[2..].to_vec();
    top_before.sort();
    top_after.sort();
    assert_eq!(top_after, top_before);
    assert!(!shuffled(&engine, "player-0"));
}
