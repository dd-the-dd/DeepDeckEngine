use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn selection_composes_quantity_predicate_and_reference_without_targets() {
    for text in [
        "one of them into",
        "up to X of those cards into",
        "any number of them into",
        "an instant or sorcery card from among them into",
        "two creature cards from among those cards into",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-selection-test", &tokens);
        let CollectionOperandAttempt::Recognized(fragment) =
            try_recognize_card_selection_operand(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(fragment.node.from, CardCollectionReferenceNode::Antecedent);
        assert_eq!(cursor.peek(), Some("into"));
        assert_eq!(fragment.end_position, cursor.position());
        assert_eq!(
            fragment.node.predicate.is_some(),
            text.contains("from among")
        );
    }
}

#[test]
fn relation_commits_only_the_selection_candidate_and_rejects_unbound_sources() {
    for text in [
        "one of target player",
        "an instant or sorcery card from among the other",
        "a creature you control from among them",
        "one of",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-selection-negative", &tokens);
        assert!(
            !matches!(
                try_recognize_card_selection_operand(&mut cursor),
                CollectionOperandAttempt::Recognized(_)
            ),
            "{text}"
        );
        assert_eq!(cursor.position(), 0);
    }
}

#[test]
fn any_number_of_reads_a_quantified_query_or_a_bound_collection() {
    for (text, filtered) in [
        ("any number of creature cards from among them and", true),
        ("any number of them and", false),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-selection-any-number", &tokens);
        let CollectionOperandAttempt::Recognized(fragment) =
            try_recognize_card_selection_operand(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(fragment.node.quantity, CardSelectionQuantityNode::AnyNumber);
        assert_eq!(fragment.node.predicate.is_some(), filtered, "{text}");
        assert_eq!(fragment.node.from, CardCollectionReferenceNode::Antecedent);
        assert_eq!(cursor.peek(), Some("and"));
    }
}

#[test]
fn restated_exile_scope_reads_the_uncast_relative_clause() {
    for (text, except_cast) in [
        ("all cards exiled this way that weren't cast into", true),
        (
            "cards exiled this way that weren't cast this way into",
            true,
        ),
        ("cards exiled this way into", false),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("collection-selection-uncast", &tokens);
        let CollectionOperandAttempt::Recognized(fragment) =
            try_recognize_card_selection_operand(&mut cursor)
        else {
            panic!("{text}")
        };
        assert_eq!(
            fragment.node.quantity,
            CardSelectionQuantityNode::All { except_cast },
            "{text}"
        );
        assert_eq!(
            fragment.node.via,
            Some(CardCollectionVerb::Exiled),
            "{text}"
        );
        assert_eq!(cursor.peek(), Some("into"), "{text}");
    }
    // A cast relative clause without its verb is not consumed.
    let tokens = recognize_entities("cards exiled this way that were into");
    let mut cursor = OracleTokenCursor::new("collection-selection-uncast-negative", &tokens);
    let CollectionOperandAttempt::Recognized(_) = try_recognize_card_selection_operand(&mut cursor)
    else {
        panic!("the restated scope is still read")
    };
    assert_eq!(cursor.peek(), Some("that"));
}
