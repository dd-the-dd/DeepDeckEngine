use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn grammatical_references_leave_the_parent_destination_untouched() {
    for (text, expected) in [
        ("them into", CardCollectionReferenceNode::Antecedent),
        ("those cards into", CardCollectionReferenceNode::Antecedent),
        ("the rest into", CardCollectionReferenceNode::Remainder),
        ("the other into", CardCollectionReferenceNode::Other),
        ("it into", CardCollectionReferenceNode::Selected),
        ("that card into", CardCollectionReferenceNode::Selected),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-reference-test", &tokens);
        let CollectionOperandAttempt::Recognized(fragment) =
            try_recognize_card_collection_reference(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(fragment.node, expected);
        assert_eq!(fragment.end_position, cursor.position());
        assert_eq!(cursor.peek(), Some("into"));
    }
}

#[test]
fn rejected_reference_trials_do_not_advance_or_bind_anything() {
    for text in [
        "that creature",
        "those spells",
        "the library",
        "draw a card",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-reference-negative", &tokens);
        assert!(!matches!(
            try_recognize_card_collection_reference(&mut cursor),
            CollectionOperandAttempt::Recognized(_)
        ));
        assert_eq!(cursor.position(), 0);
    }
}
