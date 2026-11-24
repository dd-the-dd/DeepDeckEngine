use crate::oracle::canonical::OracleTokenCursor;
use crate::oracle::syntax::recognize_entities;

use super::{lower_controller_library_search_effects, recognize_controller_library_search};

fn lower(raw: &str) -> Vec<serde_json::Value> {
    let tokens = recognize_entities(raw);
    let mut recognize = OracleTokenCursor::new("library-search-lowering-test", &tokens);
    let search = recognize_controller_library_search(&mut recognize).expect("search parses");
    let lower_tokens = recognize_entities("");
    let mut lower = OracleTokenCursor::new("library-search-lowering-test", &lower_tokens);
    lower_controller_library_search_effects(&mut lower, &search).expect("search lowers")
}

#[test]
fn hand_move_precedes_an_unexcluded_shuffle() {
    let effects =
        lower("Search your library for a card, put that card into your hand, then shuffle.");
    assert_eq!(effects[0]["kind"], "chooseCards");
    assert_eq!(effects[0]["minimum"], 1);
    assert_eq!(effects[1]["kind"], "moveCards");
    assert_eq!(effects[1]["to"]["kind"], "hand");
    assert_eq!(effects[2]["kind"], "shuffleZone");
    assert!(effects[2].get("excludeCards").is_none());
    assert_eq!(effects[3]["kind"], "recordLibrarySearch");
}

#[test]
fn revealed_qualified_card_is_excluded_from_shuffle_before_top_move() {
    let effects = lower(
        "Search your library for an artifact or enchantment card, reveal it, then shuffle and put that card on top.",
    );
    assert_eq!(effects[0]["minimum"], 0);
    assert_eq!(effects[1]["kind"], "revealCards");
    assert_eq!(effects[2]["kind"], "shuffleZone");
    assert_eq!(effects[2]["excludeCards"]["kind"], "decisionResult");
    assert_eq!(effects[2]["excludeCards"]["decisionId"], "searchedCards");
    assert_eq!(effects[3]["kind"], "moveCards");
    assert_eq!(effects[3]["to"]["kind"], "library");
    assert_eq!(effects[3]["to"]["position"], "top");
    assert_eq!(effects[4]["kind"], "recordLibrarySearch");
}

#[test]
fn typed_search_bounds_reveal_and_binding_survive_an_explicit_player() {
    for (text, minimum) in [
        (
            "Search your library for two cards, reveal them, put those cards onto the battlefield, then shuffle.",
            2,
        ),
        (
            "Search your library for two basic land cards, reveal them, put those cards onto the battlefield, then shuffle.",
            0,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut recognize = OracleTokenCursor::new("search-bound-authority", &tokens);
        let node = recognize_controller_library_search(&mut recognize).expect("typed search");
        let player = serde_json::json!({"kind":"boundValue","id":"targetController"});
        let effects = super::lower_library_search_effects(&mut recognize, &node, player.clone())
            .expect("whole node lowers for resolved player");
        assert_eq!(effects[0]["minimum"], minimum);
        assert_eq!(effects[0]["maximum"], 2);
        assert_eq!(effects[0]["player"], player);
        assert_eq!(effects[1]["kind"], "revealCards");
        assert_eq!(effects[1]["cards"], effects[2]["cards"]);
        assert_eq!(effects[3]["kind"], "shuffleZone");
        assert_eq!(effects[4]["kind"], "recordLibrarySearch");
    }
}

#[test]
fn ordinary_zone_destination_retains_search_actor_reveal_bounds_and_order() {
    let tokens = recognize_entities(
        "Search your library for two creature cards, reveal them, put those cards into your graveyard, then shuffle.",
    );
    let mut cursor = OracleTokenCursor::new("search-zone-transport", &tokens);
    let node = recognize_controller_library_search(&mut cursor).unwrap();
    let player = serde_json::json!({"kind":"boundValue", "id":"searchingPlayer"});
    let effects = super::lower_library_search_effects(&mut cursor, &node, player.clone()).unwrap();
    assert_eq!(effects.len(), 5);
    assert_eq!(effects[0]["minimum"], 0);
    assert_eq!(effects[0]["maximum"], 2);
    assert_eq!(effects[0]["player"], player);
    assert_eq!(effects[0]["candidates"]["zone"]["player"], player);
    assert_eq!(effects[1]["kind"], "revealCards");
    assert_eq!(effects[1]["cards"], effects[2]["cards"]);
    assert_eq!(
        effects[2]["to"],
        serde_json::json!({"kind":"graveyard", "player":player})
    );
    assert_eq!(effects[3]["kind"], "shuffleZone");
    assert_eq!(effects[3]["zone"]["player"], player);
    assert_eq!(effects[4]["kind"], "recordLibrarySearch");
    assert_eq!(effects[4]["player"], player);
}

#[test]
fn typed_unproved_zone_variants_fail_lowering_without_emitting_partial_actions() {
    let tokens = recognize_entities(
        "Search your library for a card, put that card into your hand, then shuffle.",
    );
    let mut cursor = OracleTokenCursor::new("search-zone-support", &tokens);
    let mut node = recognize_controller_library_search(&mut cursor).unwrap();
    // Exile is a proved search destination (`exile it, then shuffle`).
    node.destination = super::SearchDestination::Zone(super::CardZoneName::Exile);
    assert!(lower_controller_library_search_effects(&mut cursor, &node).is_ok());
    for zone in [
        super::CardZoneName::Library,
        super::CardZoneName::Battlefield,
    ] {
        node.destination = super::SearchDestination::Zone(zone);
        assert!(lower_controller_library_search_effects(&mut cursor, &node).is_err());
    }
}
