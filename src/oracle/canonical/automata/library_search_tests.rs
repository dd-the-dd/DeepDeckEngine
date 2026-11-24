use crate::oracle::canonical::{ObjectCardType, OracleTokenCursor};
use crate::oracle::syntax::recognize_entities;

use super::{
    CardZoneName, ObjectKind, ObjectPredicate, SearchDestination, SearchShuffleOrder,
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
    assert_eq!(
        search.destination,
        SearchDestination::Zone(CardZoneName::Hand)
    );
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

#[test]
fn literal_cardinality_and_plural_references_are_independent_of_the_filter() {
    for (text, minimum, maximum) in [
        (
            "Search your library for up to two basic land cards, put them onto the battlefield, then shuffle.",
            0,
            2,
        ),
        (
            "Search your library for three artifact cards, reveal those cards, put the cards into your hand, then shuffle.",
            0,
            3,
        ),
        (
            "Search your library for 4 cards, put them into your hand, then shuffle.",
            4,
            4,
        ),
        (
            "Search your library for up to one basic land card, put it onto the battlefield, then shuffle.",
            0,
            1,
        ),
    ] {
        let (search, trailing) = parse(text);
        assert_eq!((search.minimum, search.maximum), (minimum, maximum));
        assert_eq!(trailing, ".");
    }
}

#[test]
fn collection_count_and_result_number_must_agree() {
    for text in [
        "Search your library for up to two basic land card, put them onto the battlefield, then shuffle.",
        "Search your library for up to two basic land cards, put it onto the battlefield, then shuffle.",
        "Search your library for zero cards, put them into your hand, then shuffle.",
        "Search your library for 2147483648 cards, put them into your hand, then shuffle.",
        "Search your library for each basic land card, put them onto the battlefield, then shuffle.",
        "Search your library for up to two basic land cards and your opponent controls, put them onto the battlefield, then shuffle.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("search-count-near-miss", &tokens);
        assert!(
            recognize_controller_library_search(&mut cursor).is_err(),
            "{text}"
        );
    }
}

/// `X cards` is the shared typed count: the search parses with it, and the
/// face's X producer is validated after card composition (never defaulted).
#[test]
fn a_variable_search_count_is_the_shared_typed_count() {
    for text in [
        "Search your library for X cards, put them into your hand, then shuffle.",
        "Search your library for up to X basic land cards, put them onto the battlefield, then shuffle.",
    ] {
        let (search, trailing) = parse(text);
        assert!(search.count.is_some(), "{text}");
        assert!(
            search.maximum > 1,
            "{text}: the literal envelope stays plural"
        );
        assert_eq!(trailing, ".");
    }
    // `any number of` has no search maximum and stays an explicit failure.
    let tokens = recognize_entities(
        "Search your library for any number of basic land cards, put them onto the battlefield, then shuffle.",
    );
    let mut cursor = OracleTokenCursor::new("search-count-unbounded", &tokens);
    assert!(recognize_controller_library_search(&mut cursor).is_err());
}

#[test]
fn exact_real_card_structure_preflight_remains_single_primary() {
    use crate::oracle::OracleCardParseRequest;
    use crate::oracle::card_structure::{CardPartRole, CardStructureKind, plan_card_structure};
    let request = OracleCardParseRequest {
        card_name: "Harrow".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{2}{G}".to_string()),
        oracle_text: Some("As an additional cost to cast this spell, sacrifice a land.\nSearch your library for up to two basic land cards, put them onto the battlefield, then shuffle.".to_string()),
        layout: Some("normal".to_string()),
        faces: vec![],
    };
    let structure = plan_card_structure(&request).expect("complete real card structure");
    assert_eq!(structure.kind, CardStructureKind::Single);
    assert_eq!(structure.parts.len(), 1);
    assert_eq!(structure.parts[0].role, CardPartRole::Primary);
    assert_eq!(structure.parts[0].face_index, None);
}

#[test]
fn owned_zone_search_preserves_quality_bounds_and_result_number() {
    for (text, minimum, maximum, zone) in [
        (
            "Search your library for a card, put that card into your graveyard, then shuffle.",
            1,
            1,
            CardZoneName::Graveyard,
        ),
        (
            "Search your library for two creature cards, reveal them, put those cards into your graveyard, then shuffle.",
            0,
            2,
            CardZoneName::Graveyard,
        ),
        (
            "Search your library for a card, put it into your hand, then shuffle.",
            1,
            1,
            CardZoneName::Hand,
        ),
    ] {
        let (node, trailing) = parse(text);
        assert_eq!(node.destination, SearchDestination::Zone(zone));
        assert_eq!((node.minimum, node.maximum), (minimum, maximum));
        assert_eq!(node.shuffle_order, SearchShuffleOrder::MoveThenShuffle);
        assert_eq!(trailing, ".");
    }
}

#[test]
fn owned_zone_search_does_not_guess_an_owner_or_an_unproved_destination() {
    for text in [
        "Search your library for a card, put that card into their graveyard, then shuffle.",
        "Search your library for a card, put that card into your graveyards, then shuffle.",
        "Search your library for a card, put that card into your exile, then shuffle.",
        "Search your library for a card, put that card into your library, then shuffle.",
        "Search your library for a card, put that card into your battlefield, then shuffle.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("search-owned-zone-cap", &tokens);
        assert!(
            recognize_controller_library_search(&mut cursor).is_err(),
            "{text}"
        );
    }
}

#[test]
fn searched_zones_are_the_shared_zone_list_of_one_player() {
    // CR 701.23a: one possessive owns every listed zone; the library is
    // always among them, and the list keeps its written order.
    for (text, zones) in [
        (
            "Search your library and/or graveyard for a creature card, put it into your hand, then shuffle.",
            vec![CardZoneName::Library, CardZoneName::Graveyard],
        ),
        (
            "Search your graveyard, hand, and library for a card, put it into your hand, then shuffle.",
            vec![
                CardZoneName::Graveyard,
                CardZoneName::Hand,
                CardZoneName::Library,
            ],
        ),
        (
            "Search your library for a card, put it into your hand, then shuffle.",
            vec![CardZoneName::Library],
        ),
    ] {
        let (search, trailing) = parse(text);
        assert_eq!(search.origin.zones, zones, "{text}");
        assert_eq!(search.origin.holder, super::SearchedPlayer::Searcher);
        assert_eq!(trailing, ".");
    }
    // Near-misses: a search names a library, and only its player's own
    // hidden or public card zones; nothing is silently dropped.
    for (text, state) in [
        (
            "Search your graveyard for a card, put it into your hand, then shuffle.",
            "effect.library-search.origin-library",
        ),
        (
            "Search your library and exile for a card, put it into your hand, then shuffle.",
            "effect.library-search.origin-library",
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("search-origin-near-miss", &tokens);
        let failure = recognize_controller_library_search(&mut cursor).expect_err(text);
        assert_eq!(failure.state, state, "{text}: {failure:?}");
    }
}

#[test]
fn found_cards_can_be_exiled_and_a_declared_holder_shuffles_later() {
    let (search, trailing) = parse("Search your library for a card, exile it, then shuffle.");
    assert_eq!(
        search.destination,
        SearchDestination::Zone(CardZoneName::Exile)
    );
    assert_eq!(search.shuffle_order, SearchShuffleOrder::MoveThenShuffle);
    assert_eq!(trailing, ".");

    // Jester's Cap / Bitter Ordeal: the possessive declares the holder; the
    // shuffle is that player's own later instruction.
    let (search, trailing) =
        parse("Search target player's library for three cards and exile them.");
    assert!(matches!(
        search.origin.holder,
        super::SearchedPlayer::DeclaredTarget(_)
    ));
    assert_eq!((search.minimum, search.maximum), (3, 3));
    assert_eq!(
        search.destination,
        SearchDestination::Zone(CardZoneName::Exile)
    );
    assert_eq!(
        search.shuffle_order,
        SearchShuffleOrder::ByHolderInstruction
    );
    assert_eq!(trailing, ".");

    // Near-miss: the searcher's own library is shuffled by the search.
    let tokens = recognize_entities("Search your library for a card and exile it.");
    let mut cursor = OracleTokenCursor::new("search-exile-near-miss", &tokens);
    let failure = recognize_controller_library_search(&mut cursor).expect_err("own library");
    assert_eq!(failure.state, "effect.library-search.exile-completion");
}
