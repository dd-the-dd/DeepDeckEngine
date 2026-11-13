use crate::oracle::canonical::{
    ObjectCardType, ObjectControllerScope, ObjectKind, ObjectPredicate,
};
use crate::oracle::syntax::recognize_entities;

use super::{
    CounterProhibitionSubjectNode, LocalObjectReference, OracleTokenCursor,
    recognize_counter_prohibition,
};

#[test]
fn self_spell_counter_prohibition_is_typed_and_fully_consumed() {
    let tokens = recognize_entities("This spell can't be countered.");
    let mut cursor = OracleTokenCursor::new("counter-prohibition-test", &tokens);

    let node = recognize_counter_prohibition(&mut cursor).expect("prohibition parses");

    assert_eq!(
        node.subject,
        CounterProhibitionSubjectNode::SourceSelf(LocalObjectReference::SourceSelf)
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn controlled_creature_spells_are_a_typed_filtered_stack_set() {
    let tokens = recognize_entities("Creature spells you control can't be countered.");
    let mut cursor = OracleTokenCursor::new("counter-prohibition-test", &tokens);
    let node = recognize_counter_prohibition(&mut cursor).expect("prohibition parses");

    let CounterProhibitionSubjectNode::ObjectSet(subject) = node.subject else {
        panic!("expected an object set")
    };
    assert_eq!(
        subject.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert_eq!(
        subject.predicate,
        ObjectPredicate::Intersection(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::ObjectKind(ObjectKind::Spell),
        ])
    );
}

#[test]
fn wrong_passive_action_fails_at_the_action_token() {
    let tokens = recognize_entities("This spell can't be copied.");
    let mut cursor = OracleTokenCursor::new("counter-prohibition-test", &tokens);

    let failure = recognize_counter_prohibition(&mut cursor)
        .expect_err("copy prohibition is a different grammar");

    assert_eq!(failure.state, "counter-prohibition.action");
    assert_eq!(failure.actual.as_deref(), Some("copied"));
}
