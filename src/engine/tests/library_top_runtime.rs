//! Runtime proof of the top-of-library card-collection operand: `reveal`, `exile`
//! and `put` consume one bound set, and a player permission reads that same set.
use super::*;

fn spell_effects(text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{1}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{text}");
    parsed.abilities[0].rule.as_ref().expect("a canonical rule")["effects"]
        .as_array()
        .expect("spell effects")
        .clone()
}

fn library_card(id: &str, type_line: &str) -> CardInstance {
    let mut definition = test_definition(id, type_line);
    if type_line == "Instant" {
        definition.rules = vec![json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [{
                "kind": "gainLife",
                "player": { "kind": "abilityController" },
                "amount": 1
            }]
        })];
    }
    test_instance(id, definition, "player-0")
}

/// Three cards; `top` is the last element, which is the top of the library.
fn engine_with_library() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.players[0].hand.clear();
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.players[0].library = vec![
        library_card("deep", "Creature - Elf"),
        library_card("second", "Instant"),
        library_card("top", "Instant"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture library cards are admitted with an incarnation");
    engine
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

fn playable_from_exile(engine: &GameEngine, card: &str) -> bool {
    engine.legal_priority_actions(0).iter().any(|action| {
        action.kind == ActionKind::CastSpell
            && action.card_instance_id.as_deref() == Some(card)
            && action.decisions["castSourceZone"] == "exile"
    })
}

/// Impulse draw (CR 406.3): the exiled card, not whatever is on top afterwards,
/// is playable this turn and the permission ends in cleanup.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn exiled_top_card_is_playable_this_turn_then_the_permission_expires() {
    let effects = spell_effects(
        "Exile the top card of your library. Until end of turn, you may play that card.",
    );
    let mut engine = engine_with_library();
    resolve(&mut engine, &effects);

    assert_eq!(ids(&engine.state.players[0].library), ["deep", "second"]);
    assert_eq!(ids(&engine.state.players[0].exile), ["top"]);
    assert_eq!(engine.state.permissions.len(), 1);
    assert_eq!(engine.state.permissions[0]["cardInstanceId"], "top");
    assert!(playable_from_exile(&engine, "top"));
    assert!(!playable_from_exile(&engine, "second"));

    engine
        .cleanup(&mut EmeritusDecisionProvider)
        .expect("cleanup expires this-turn permissions");
    assert!(engine.state.permissions.is_empty());
    assert!(!playable_from_exile(&engine, "top"));
}

/// A sibling combination of the same concepts: several cards, a trailing
/// window, and the end of the controller's next turn (CR 514.2).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn several_exiled_cards_share_a_window_through_the_end_of_the_next_turn() {
    let effects = spell_effects(
        "Exile the top two cards of your library. You may play those cards until the end of your next turn.",
    );
    let mut engine = engine_with_library();
    resolve(&mut engine, &effects);

    assert_eq!(ids(&engine.state.players[0].library), ["deep"]);
    let mut exiled = ids(&engine.state.players[0].exile);
    exiled.sort_unstable();
    assert_eq!(exiled, ["second", "top"]);
    let next_turn = engine
        .next_turn_number_for_player("player-0")
        .expect("player-0 has a next turn");
    assert_eq!(engine.state.permissions.len(), 2);
    for permission in &engine.state.permissions {
        assert_eq!(permission["expiresAfterTurn"], next_turn);
    }
    assert!(playable_from_exile(&engine, "top"));
    assert!(playable_from_exile(&engine, "second"));
}

/// Reveal is a verb on the same collection: the revealed set is the set moved.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn revealed_top_card_is_the_card_put_into_hand() {
    let effects =
        spell_effects("Reveal the top card of your library and put that card into your hand.");
    let mut engine = engine_with_library();
    resolve(&mut engine, &effects);

    assert_eq!(ids(&engine.state.players[0].hand), ["top"]);
    assert_eq!(ids(&engine.state.players[0].library), ["deep", "second"]);
    let revealed = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "cardsRevealed")
        .expect("the card is revealed");
    assert_eq!(revealed.detail["cardIds"], json!(["top"]));
}

/// A request larger than the library is bounded by the library, never an error.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn putting_more_cards_than_the_library_holds_moves_only_what_exists() {
    let effects = spell_effects("Put the top five cards of your library into your graveyard.");
    let mut engine = engine_with_library();
    resolve(&mut engine, &effects);

    assert!(engine.state.players[0].library.is_empty());
    let mut graveyard = ids(&engine.state.players[0].graveyard);
    graveyard.sort_unstable();
    assert_eq!(graveyard, ["deep", "second", "top"]);
}

/// Only the requested prefix of the library is selected.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_bounded_top_selection_leaves_the_rest_of_the_library_untouched() {
    let effects = spell_effects("Put the top two cards of your library into your graveyard.");
    let mut engine = engine_with_library();
    resolve(&mut engine, &effects);

    assert_eq!(ids(&engine.state.players[0].library), ["deep"]);
    assert_eq!(engine.state.players[0].graveyard.len(), 2);
}

/// `all X cards revealed this way` and `the rest` partition one fixed snapshot:
/// the filter is evaluated on the revealed identities, even after some moved.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn matching_revealed_cards_and_the_rest_partition_one_snapshot() {
    let effects = spell_effects(
        "Reveal the top three cards of your library. Put all creature cards revealed this way into your hand and the rest into your graveyard.",
    );
    let mut engine = engine_with_library();
    resolve(&mut engine, &effects);

    assert_eq!(ids(&engine.state.players[0].hand), ["deep"]);
    let mut graveyard = ids(&engine.state.players[0].graveyard);
    graveyard.sort_unstable();
    assert_eq!(graveyard, ["second", "top"]);
    assert!(engine.state.players[0].library.is_empty());
}

/// `reveal until` observes every revealed card (CR 701.20a); the matching card
/// moves, and the rest of the same observation goes to the bottom.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn bounded_reveal_stops_at_the_match_and_keeps_the_unrevealed_library_on_top() {
    let effects = spell_effects(
        "Reveal cards from the top of your library until you reveal a land card. Put that card into your hand and the rest on the bottom of your library in a random order.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = vec![
        library_card("unrevealed", "Instant"),
        library_card("land", "Basic Land - Forest"),
        library_card("creature", "Creature - Elf"),
        library_card("spell", "Instant"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture library cards are admitted with an incarnation");
    resolve(&mut engine, &effects);

    assert_eq!(ids(&engine.state.players[0].hand), ["land"]);
    // The unrevealed card is still on top; the two missed cards sit below it.
    let library = ids(&engine.state.players[0].library);
    assert_eq!(library.len(), 3);
    assert_eq!(library.last(), Some(&"unrevealed"));
    let revealed = engine
        .state
        .events
        .iter()
        .find(|event| event.kind == "cardsRevealed")
        .expect("the cards are revealed");
    assert_eq!(
        revealed.detail["cardIds"],
        json!(["spell", "creature", "land"])
    );
}
