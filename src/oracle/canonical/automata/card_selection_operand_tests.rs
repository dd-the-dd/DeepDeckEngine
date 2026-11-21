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
