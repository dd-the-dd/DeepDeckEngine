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
