use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn destination_fields_are_independent_and_parent_owns_connectors() {
    for (text, zone, position, order, tapped) in [
        ("into your hand and", CardZoneName::Hand, None, None, false),
        (
            "into your graveyard and",
            CardZoneName::Graveyard,
            None,
            None,
            false,
        ),
        ("to your exile and", CardZoneName::Exile, None, None, false),
        (
            "on the bottom of your library in any order and",
            CardZoneName::Library,
            Some(LibraryPosition::Bottom),
            Some(CollectionOrder::Chosen),
            false,
        ),
        (
            "on top of your library in a random order and",
            CardZoneName::Library,
            Some(LibraryPosition::Top),
            Some(CollectionOrder::Random),
            false,
        ),
        (
            "onto the battlefield tapped and",
            CardZoneName::Battlefield,
            None,
            None,
            true,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-destination-test", &tokens);
        let CollectionOperandAttempt::Recognized(fragment) =
            try_recognize_card_move_destination(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(
            (
                fragment.node.zone,
                fragment.node.position,
                fragment.node.order,
                fragment.node.tapped
            ),
            (zone, position, order, tapped)
        );
        assert_eq!(cursor.peek(), Some("and"));
    }
}

#[test]
fn destination_does_not_guess_owner_position_or_order_domain() {
    for text in [
        "into exile",
        "on your library",
        "on top of your hand",
        "into your hand in any order",
        "onto the battlefield in any order",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-destination-negative", &tokens);
        assert!(
            matches!(
                try_recognize_card_move_destination(&mut cursor),
                CollectionOperandAttempt::FailedCommitted(_)
            ),
            "{text}"
        );
        assert_eq!(cursor.position(), 0);
    }
}
