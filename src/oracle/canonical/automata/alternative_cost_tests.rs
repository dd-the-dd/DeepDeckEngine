use crate::oracle::syntax::recognize_entities;

use super::super::super::OracleTokenCursor;
use super::super::{NumericComparisonOperator, ObjectPredicate, controlled_permanent_count};
use super::*;

#[test]
fn conditional_free_cast_keeps_condition_and_zero_cost_independent() {
    let tokens = recognize_entities(
        "If you control a commander, you may cast this spell without paying its mana cost.",
    );
    let mut cursor = OracleTokenCursor::new("test.conditional-free-cast", &tokens);

    let envelope = recognize_conditional_free_cast_envelope(&mut cursor)
        .expect("conditional free cast parses");

    assert!(envelope.costs.costs.is_empty());
    assert!(envelope.costs.connectors.is_empty());
    let condition = envelope
        .condition
        .expect("controlled commander condition remains typed");
    let (predicate, operator, threshold, exclude_source) =
        controlled_permanent_count(&condition).expect("a controlled permanent count");
    assert_eq!(operator, NumericComparisonOperator::GreaterThanOrEqual);
    assert_eq!(threshold, 1);
    assert_eq!(*predicate, ObjectPredicate::IsCommander);
    assert!(!exclude_source);
    assert_eq!(cursor.position(), tokens.len());
}
