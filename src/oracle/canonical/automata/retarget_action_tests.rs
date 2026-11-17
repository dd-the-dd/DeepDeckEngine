use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

#[test]
fn retarget_family_is_transactional_and_rejects_nonstack_targets() {
    let tokens = recognize_oracle_entities("Draw a card.");
    let mut cursor = OracleTokenCursor::new("test.retarget", &tokens);
    assert!(matches!(
        try_recognize_retarget_action(&mut cursor),
        RetargetActionAttempt::NotClaimed
    ));
    assert_eq!(cursor.position(), 0);
    let tokens = recognize_oracle_entities("Choose new targets for target creature.");
    let mut cursor = OracleTokenCursor::new("test.retarget", &tokens);
    match try_recognize_retarget_action(&mut cursor) {
        RetargetActionAttempt::FailedCommitted(failure) => {
            assert_eq!(failure.state, "effect.retarget.stack-domain")
        }
        _ => panic!("introduced action must return its committed domain failure"),
    };
    assert_eq!(cursor.position(), 0);
}

#[test]
fn shared_stack_union_and_outer_sequence_boundary_are_preserved() {
    let tokens = recognize_oracle_entities(
        "Choose new targets for target spell or ability, then draw a card.",
    );
    let mut cursor = OracleTokenCursor::new("test.retarget", &tokens);
    let RetargetActionAttempt::Recognized(node) = try_recognize_retarget_action(&mut cursor) else {
        panic!("heterogeneous target should parse");
    };
    assert!(
        matches!(&node.target.slots[0].selector,TargetSelector::StackItem(StackItemPredicate::Union(branches)) if branches.len()==2)
    );
    assert_eq!(cursor.peek(), Some(","));
    let tokens = recognize_oracle_entities("Choose new targets for target spell.");
    let mut cursor = OracleTokenCursor::new("test.retarget", &tokens);
    let RetargetActionAttempt::Recognized(node) = try_recognize_retarget_action(&mut cursor) else {
        panic!("spell target should parse");
    };
    assert!(
        matches!(node.target.slots[0].selector, TargetSelector::Object(_)),
        "old plain-spell selector keeps its existing IR domain"
    );
}
