//! Real Oracle text -> primitive IR -> engine behavior for the object-to-zone
//! frame over observed card collections: an elided library position puts
//! `the rest` back into the library the cards were observed in, in an order
//! the engine's random source decides (CR 401.4); `shuffle the cards from your
//! hand into your library` sends each card to its owner's library (CR 400.3,
//! 701.24b); and an `If you do` body reads the card its producing exile
//! observed, even from a delayed trigger after the source left (CR 603.7c,
//! 608.2c).
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{U}".to_string()),
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
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn spell_effects(oracle: &str) -> Vec<Value> {
    let rules = parsed_rules("Probe", "Sorcery", oracle);
    rules[0]["effects"]
        .as_array()
        .expect("spell effects")
        .clone()
}

fn resolve(engine: &mut GameEngine, effects: &[Value]) {
    let source = test_instance("probe", test_definition("probe", "Sorcery"), "player-0");
    let stack = StackObject {
        id: "stack:probe".to_string(),
        controller: "player-0".to_string(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("spellAbility".to_string()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    for effect in effects {
        assert!(effect_supported(effect), "{effect:#}");
    }
    engine
        .execute_effects(
            &Value::Array(effects.to_vec()),
            &stack,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            &mut EmeritusDecisionProvider,
        )
        .expect("the program resolves");
}

fn ids(cards: &[CardInstance]) -> Vec<&str> {
    cards.iter().map(|card| card.instance_id.as_str()).collect()
}

fn card(id: &str, type_line: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), owner)
}

/// `the rest on the bottom in a random order` with no written library: the
/// cards go back to the library they were looked at in, under the cards that
/// were never looked at, in an order the engine's seeded random source
/// decides. The order varies with the seed; the placed set never does.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_rest_on_the_bottom_in_a_random_order_follows_the_engine_random_source() {
    let effects = spell_effects(
        "Look at the top four cards of your library. Put one of them into your hand and the rest on the bottom in a random order.",
    );
    let mut orders = std::collections::BTreeSet::new();
    for seed in 0..24u64 {
        let mut engine = test_engine(2);
        engine.rng = StdRng::seed_from_u64(seed);
        engine.state.players[0].hand.clear();
        // The library's top is its last element: `unseen` stays on top.
        engine.state.players[0].library = vec![
            card("unseen", "Instant", "player-0"),
            card("a", "Instant", "player-0"),
            card("b", "Creature - Elf", "player-0"),
            card("c", "Artifact", "player-0"),
            card("d", "Sorcery", "player-0"),
        ];
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture library cards are admitted");
        resolve(&mut engine, &effects);

        let hand = ids(&engine.state.players[0].hand);
        assert_eq!(hand.len(), 1, "one looked-at card is put into the hand");
        let library = ids(&engine.state.players[0].library);
        assert_eq!(library.len(), 4);
        assert_eq!(
            library.last(),
            Some(&"unseen"),
            "the unseen card stays on top"
        );
        let mut placed = library[..3].to_vec();
        let order = placed.join(",");
        placed.sort_unstable();
        let mut expected = ["a", "b", "c", "d"]
            .into_iter()
            .filter(|id| !hand.contains(id))
            .collect::<Vec<_>>();
        expected.sort_unstable();
        assert_eq!(placed, expected, "exactly the rest is on the bottom");
        orders.insert(order);
    }
    assert!(
        orders.len() > 1,
        "the random order is drawn from the engine's random source"
    );
}

/// `shuffle the cards from your hand into your library`: every card of the
/// hand goes to its owner's library, even a card another player owns, and
/// each library that received a card is shuffled.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn shuffling_a_hand_into_a_library_sends_each_card_to_its_owners_library() {
    let effects = spell_effects("Shuffle the cards from your hand into your library.");
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![
        card("mine", "Instant", "player-0"),
        card("borrowed", "Creature - Bear", "player-1"),
    ];
    let own_before = engine.state.players[0].library.len();
    let other_before = engine.state.players[1].library.len();
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture hand cards are admitted");
    let events_before = engine.state.events.len();
    resolve(&mut engine, &effects);

    assert!(engine.state.players[0].hand.is_empty());
    assert_eq!(engine.state.players[0].library.len(), own_before + 1);
    assert_eq!(engine.state.players[1].library.len(), other_before + 1);
    assert!(ids(&engine.state.players[0].library).contains(&"mine"));
    assert!(ids(&engine.state.players[1].library).contains(&"borrowed"));
    let mut shuffled = engine.state.events[events_before..]
        .iter()
        .filter(|event| event.kind == "zoneShuffled")
        .filter_map(|event| event.player_id.clone())
        .collect::<Vec<_>>();
    shuffled.sort_unstable();
    assert_eq!(shuffled, ["player-0", "player-1"]);
}

/// `the rest of the revealed cards` after `all creature cards revealed this
/// way`: the restated selection reads the whole revealed collection, so the
/// rest is exactly the noncreature cards revealed (Mass Polymorph's shape).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn the_rest_of_the_revealed_cards_is_the_complement_of_the_restated_selection() {
    let effects = spell_effects(
        "Reveal cards from the top of your library until you reveal two creature cards. Put all creature cards revealed this way into your hand, then put the rest of the revealed cards into your graveyard.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = vec![
        card("unrevealed", "Instant", "player-0"),
        card("elf", "Creature - Elf", "player-0"),
        card("rock", "Artifact", "player-0"),
        card("bear", "Creature - Bear", "player-0"),
        card("bolt", "Instant", "player-0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture library cards are admitted");
    resolve(&mut engine, &effects);

    let mut hand = ids(&engine.state.players[0].hand);
    hand.sort_unstable();
    assert_eq!(hand, ["bear", "elf"]);
    let mut graveyard = ids(&engine.state.players[0].graveyard);
    graveyard.sort_unstable();
    assert_eq!(graveyard, ["bolt", "rock"]);
    assert_eq!(ids(&engine.state.players[0].library), ["unrevealed"]);
}

/// `exile another target creature. If you do, return that card to the
/// battlefield ... at the beginning of the next end step`: the outcome body
/// reads the card its producing exile observed. The delayed return carries
/// that card, so it returns even after the ability's source left.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_if_you_do_return_reads_the_exiled_card_after_the_source_left() {
    let oracle = "When this creature enters, exile another target creature. If you do, return that card to the battlefield under its owner's control at the beginning of the next end step.";
    let mut definition = test_definition("slider", "Creature — Spirit");
    definition.name = "Slider".to_string();
    definition.rules = parsed_rules("Slider", "Creature — Spirit", oracle);
    let source = test_instance("slider", definition, "player-0");
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield.push(source.clone());
    engine.state.players[1]
        .battlefield
        .push(card("captive", "Creature — Bear", "player-1"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let (player, index) = engine
        .permanent_position("slider")
        .expect("the source entered");
    let live = engine.state.players[player].battlefield[index].clone();
    engine.enqueue_enter_battlefield_triggers(&live);
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the entry trigger resolves");
    assert!(engine.permanent_position("captive").is_none());
    assert!(
        engine.state.players[1]
            .exile
            .iter()
            .any(|card| card.instance_id == "captive")
    );

    engine
        .move_permanent_to_graveyard("slider", "destroyed")
        .expect("the source leaves the battlefield");
    engine.enqueue_step_triggers("endStep");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the delayed return resolves");
    let (player, _) = engine
        .permanent_position("captive")
        .expect("the exiled card returned");
    assert_eq!(player, 1, "under its owner's control");
}
