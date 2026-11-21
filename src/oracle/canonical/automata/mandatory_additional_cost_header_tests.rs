use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn the_same_header_leaves_each_payment_action_unconsumed() {
    for action in ["discard a card.", "sacrifice a land.", "pay 2 life."] {
        let text = format!("As an additional cost to cast this spell, {action}");
        let tokens = recognize_entities(&text);
        assert!(has_mandatory_additional_cost_header(&tokens));
        let mut cursor = OracleTokenCursor::new("shared-cost-header-test", &tokens);
        cursor.enter("caller.retains-diagnostic-state");
        recognize_mandatory_additional_cost_header(&mut cursor).expect("the shared header");
        assert_eq!(cursor.position(), MANDATORY_ADDITIONAL_COST_HEADER.len());
        assert_eq!(cursor.peek(), action.split_whitespace().next());
        let failure = cursor.fail("a caller-owned operand");
        assert_eq!(failure.state, "caller.retains-diagnostic-state");
    }
}

#[test]
fn optional_casting_and_activation_envelopes_are_not_mandatory_headers() {
    for text in [
        "As an additional cost to cast this spell you may discard a card.",
        "You may discard a card rather than pay this spell's mana cost.",
        "Discard a card: Draw a card.",
    ] {
        let tokens = recognize_entities(text);
        assert!(!has_mandatory_additional_cost_header(&tokens));
        let mut cursor = OracleTokenCursor::new("shared-cost-header-test", &tokens);
        assert!(recognize_mandatory_additional_cost_header(&mut cursor).is_err());
    }
}
