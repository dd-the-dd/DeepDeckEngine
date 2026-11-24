use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

#[test]
fn ordinary_subject_borrows_the_shared_discard_action_and_card_quantity() {
    for text in [
        "Discard a card, then",
        "You discard two cards, then",
        "Discard 3 cards at random, then",
        "Discard X cards, then",
        "Discard your hand, then",
        "Discard all the cards in your hand, then",
    ] {
        let tokens = recognize_oracle_entities(text);
        let mut cursor = OracleTokenCursor::new("test.ordinary-discard", &tokens);
        let node = recognize_ordinary_player_discard(&mut cursor).unwrap();
        assert_eq!(node.player, PlayerEffectSubject::Controller);
        assert_eq!(
            cursor.peek(),
            Some(","),
            "child must leave connector: {text}"
        );
    }
}

#[test]
fn ordinary_discard_rejects_malformed_counts_nouns_and_selection_without_consuming_a_sentence() {
    for text in [
        "Discard one cards.",
        "Discard two card.",
        "Discard a creature.",
        "Discard a card at choice.",
        "You discards a card.",
        "Discard -1 cards.",
    ] {
        let tokens = recognize_oracle_entities(text);
        let mut cursor = OracleTokenCursor::new("test.ordinary-discard", &tokens);
        assert!(
            recognize_ordinary_player_discard(&mut cursor).is_err(),
            "{text}"
        );
    }
}
