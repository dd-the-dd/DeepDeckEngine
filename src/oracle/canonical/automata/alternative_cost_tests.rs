use crate::oracle::syntax::recognize_entities;

use super::super::super::OracleTokenCursor;
use super::super::{
    ControlledCountDirection, LocalConditionNode, LocalPlayerReference, ObjectPredicate,
};
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
    let Some(LocalConditionNode::ControlledPermanentCount(condition)) = envelope.condition else {
        panic!("controlled commander condition remains typed");
    };
    assert_eq!(condition.player, LocalPlayerReference::Controller);
    assert_eq!(condition.threshold, 1);
    assert_eq!(condition.direction, ControlledCountDirection::OrMore);
    assert_eq!(condition.predicate, ObjectPredicate::IsCommander);
    assert!(!condition.exclude_source);
    assert_eq!(cursor.position(), tokens.len());
}
