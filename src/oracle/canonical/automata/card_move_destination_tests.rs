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

/// The entry-modifier list is read in any written order after a battlefield
/// destination; `and` joins two modifiers and otherwise stays with the parent.
#[test]
fn entry_modifiers_compose_in_any_order_and_leave_foreign_connectors() {
    let tokens = recognize_entities(
        "onto the battlefield tapped and transformed under your control with a +1/+1 counter on it and",
    );
    let mut cursor = OracleTokenCursor::new("entry-modifiers", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_card_move_destination(&mut cursor)
    else {
        panic!("a modified battlefield destination")
    };
    let node = fragment.node;
    assert!(node.tapped && node.transformed && !node.face_down);
    assert_eq!(
        node.owner,
        CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
    );
    assert!(node.entering_counters.is_some());
    assert_eq!(cursor.peek(), Some("and"));

    let tokens = recognize_entities("to the battlefield face down under its owner's control.");
    let mut cursor = OracleTokenCursor::new("entry-modifiers-face-down", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_card_move_destination(&mut cursor)
    else {
        panic!("a face-down battlefield destination")
    };
    assert!(fragment.node.face_down && !fragment.node.transformed);
    assert_eq!(fragment.node.owner, CardMoveOwnerNode::EachCardOwner);

    let tokens = recognize_entities("onto the battlefield tapped and attacking");
    let mut cursor = OracleTokenCursor::new("entry-modifiers-boundary", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_card_move_destination(&mut cursor)
    else {
        panic!("a tapped battlefield destination")
    };
    assert!(fragment.node.tapped);
    assert_eq!(cursor.peek(), Some("and"));
}

/// Mutually exclusive or repeated modifiers fail explicitly; a bare player
/// possessive names only an owned hand, library or graveyard (CR 400.3).
#[test]
fn exclusive_modifiers_and_bare_possessives_fail_explicitly() {
    for text in [
        "onto the battlefield face down and transformed",
        "onto the battlefield tapped and tapped",
        "to their battlefield",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("entry-modifiers-negative", &tokens);
        assert!(
            matches!(
                try_recognize_card_move_destination(&mut cursor),
                CollectionOperandAttempt::FailedCommitted(_)
            ),
            "{text}"
        );
        assert_eq!(cursor.position(), 0, "{text}");
    }
    let tokens = recognize_entities("to their hand.");
    let mut cursor = OracleTokenCursor::new("bare-possessive", &tokens);
    let CollectionOperandAttempt::Recognized(fragment) =
        try_recognize_card_move_destination(&mut cursor)
    else {
        panic!("an owned hand")
    };
    assert_eq!(fragment.node.zone, CardZoneName::Hand);
    assert_eq!(fragment.node.owner, CardMoveOwnerNode::EachCardOwner);
}
