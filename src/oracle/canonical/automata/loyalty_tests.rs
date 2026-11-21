use crate::oracle::syntax::recognize_entities;

use super::super::LoyaltyCostNode;
use super::{OracleTokenCursor, recognize_loyalty_cost};

#[test]
fn positive_negative_and_zero_loyalty_costs_share_one_machine() {
    for (text, expected) in [("+2", 2), ("\u{2212}3", -3), ("-7", -7), ("0", 0)] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("loyalty-cost-test", &tokens);

        assert_eq!(
            recognize_loyalty_cost(&mut cursor).expect("literal loyalty cost parses"),
            LoyaltyCostNode { delta: expected }
        );
        assert_eq!(cursor.position(), tokens.len());
    }
}

#[test]
fn variable_loyalty_is_left_for_a_distinct_numeric_contract() {
    let tokens = recognize_entities("−X");
    let mut cursor = OracleTokenCursor::new("loyalty-cost-test", &tokens);

    let failure = recognize_loyalty_cost(&mut cursor)
        .expect_err("literal loyalty machine must not approximate X");

    assert_eq!(failure.actual.as_deref(), Some("X"));
    assert!(failure.expected.contains("literal loyalty magnitude"));
}
