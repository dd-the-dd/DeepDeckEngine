use crate::oracle::canonical::{ObjectCardType, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

use super::{
    ObjectKind, ObjectPredicate, SearchDestination, SearchShuffleOrder,
    recognize_controller_library_search,
};

fn parse(raw: &str) -> (super::LibrarySearchEffectNode, String) {
    let tokens = recognize_entities(raw);
    let mut cursor = OracleTokenCursor::new("library-search-test", &tokens);
    let node = recognize_controller_library_search(&mut cursor).expect("search parses");
    (node, cursor.peek().unwrap_or_default().to_string())
}

#[test]
fn bare_card_to_hand_is_mandatory_when_a_candidate_exists() {
    let (search, trailing) =
        parse("Search your library for a card, put that card into your hand, then shuffle.");
    assert_eq!(search.filter, ObjectPredicate::ObjectKind(ObjectKind::Card));
    assert_eq!(search.minimum, 1);
    assert_eq!(search.maximum, 1);
    assert_eq!(search.destination, SearchDestination::Hand);
    assert!(!search.reveal);
    assert_eq!(search.shuffle_order, SearchShuffleOrder::MoveThenShuffle);
    assert_eq!(trailing, ".");
}

#[test]
fn qualified_union_can_fail_and_reveals_before_shuffle_then_top_move() {
    let (search, trailing) = parse(
        "Search your library for an artifact or enchantment card, reveal it, then shuffle and put that card on top.",
    );
    assert_eq!(
        search.filter,
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ])
    );
    assert_eq!(search.minimum, 0);
    assert_eq!(search.destination, SearchDestination::LibraryTop);
    assert!(search.reveal);
    assert_eq!(search.shuffle_order, SearchShuffleOrder::ShuffleThenMove);
    assert_eq!(trailing, ".");
}

#[test]
fn battlefield_destination_keeps_existing_move_then_shuffle_shape() {
    let (search, trailing) = parse(
        "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.",
    );
    assert_eq!(
        search.destination,
        SearchDestination::Battlefield { tapped: true }
    );
    assert_eq!(search.minimum, 0);
    assert!(!search.reveal);
    assert_eq!(search.shuffle_order, SearchShuffleOrder::MoveThenShuffle);
    assert_eq!(trailing, ".");
}

#[test]
fn library_top_destination_requires_shuffle_before_the_move() {
    let tokens =
        recognize_entities("Search your library for a card, put that card on top, then shuffle.");
    let mut cursor = OracleTokenCursor::new("library-search-near-miss", &tokens);
    let failure = recognize_controller_library_search(&mut cursor)
        .expect_err("moving to the top before shuffling changes the selected top card");
    assert_eq!(failure.state, "effect.library-search.destination");
    assert!(
        failure
            .expected
            .contains("library-top move after shuffling")
    );
}
