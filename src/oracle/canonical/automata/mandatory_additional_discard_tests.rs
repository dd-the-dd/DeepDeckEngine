use super::*;
use crate::oracle::canonical::automata::ObjectCardType;
use crate::oracle::syntax::recognize_entities;

fn parsed(text: &str) -> MandatoryAdditionalDiscardCostNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("additional-discard-child-test", &tokens);
    match try_recognize_mandatory_additional_discard_cost(&mut cursor) {
        MandatoryAdditionalDiscardCostAttempt::Parsed(node) => node,
        other => panic!("complete discard cost must parse: {other:?}"),
    }
}

#[test]
fn articles_digits_and_number_words_share_one_counted_card_action() {
    for quantity in ["a", "one", "1"] {
        let node = parsed(&format!(
            "As an additional cost to cast this spell, discard {quantity} card."
        ));
        assert_eq!(node.quantity, 1);
        assert!(node.filter.supports_domain(ObjectKind::Card));
        assert_eq!(node.selection, CollectionSelectionMethod::Choice);
    }
    for quantity in ["two", "2"] {
        assert_eq!(
            parsed(&format!(
                "As an additional cost to cast this spell, discard {quantity} cards."
            ))
            .quantity,
            2
        );
    }
}

#[test]
fn the_card_query_retains_a_payment_filter() {
    let node = parsed("As an additional cost to cast this spell, discard a land card.");
    assert!(node.filter.guarantees_card_type(ObjectCardType::Land));
    assert_eq!(node.quantity, 1);
}

#[test]
fn unsupported_random_quantity_and_suffix_restore_the_transaction() {
    for text in [
        "As an additional cost to cast this spell, discard a card at random.",
        "As an additional cost to cast this spell, discard X cards.",
        "As an additional cost to cast this spell, discard 65 cards.",
        "As an additional cost to cast this spell, discard a card that is red.",
        "As an additional cost to cast this spell, discard a card. Draw a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("additional-discard-child-test", &tokens);
        let MandatoryAdditionalDiscardCostAttempt::Unpromoted(failure) =
            try_recognize_mandatory_additional_discard_cost(&mut cursor)
        else {
            panic!("unsupported payment must remain unpromoted: {text}");
        };
        assert!(!failure.expected.is_empty());
        assert_eq!(cursor.position(), 0);
        assert!(
            failure
                .trace
                .iter()
                .any(|step| step.state == "action.discard.verb")
        );
    }
}

#[test]
fn malformed_nonpositive_quantity_commits_a_diagnostic() {
    let tokens = recognize_entities("As an additional cost to cast this spell, discard 0 cards.");
    let mut cursor = OracleTokenCursor::new("additional-discard-child-test", &tokens);
    let MandatoryAdditionalDiscardCostAttempt::Failed(failure) =
        try_recognize_mandatory_additional_discard_cost(&mut cursor)
    else {
        panic!("a zero-card mandatory payment is malformed");
    };
    assert_eq!(failure.expected, "a positive discard-card quantity");
}

#[test]
fn noun_number_agreement_and_unbound_query_variables_do_not_promote() {
    for text in [
        "As an additional cost to cast this spell, discard two card.",
        "As an additional cost to cast this spell, discard a cards.",
        "As an additional cost to cast this spell, discard a creature card with mana value X.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("additional-discard-child-test", &tokens);
        assert!(matches!(
            try_recognize_mandatory_additional_discard_cost(&mut cursor),
            MandatoryAdditionalDiscardCostAttempt::Unpromoted(_)
        ));
        assert_eq!(cursor.position(), 0);
    }
}
