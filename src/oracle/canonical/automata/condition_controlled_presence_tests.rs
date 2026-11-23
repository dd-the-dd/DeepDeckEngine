use crate::oracle::syntax::recognize_entities;

use super::super::super::OracleTokenCursor;
use super::super::{ObjectCardType, ObjectPredicate};
use super::*;

#[test]
fn commander_presence_is_the_shared_controlled_permanent_comparison() {
    let tokens = recognize_entities("If you control a commander,");
    let mut cursor = OracleTokenCursor::new("test.controlled-permanent-presence", &tokens);

    let condition = recognize_controlled_permanent_presence_condition(&mut cursor)
        .expect("commander presence condition parses");

    assert_eq!(
        condition,
        BooleanConditionNode::Atom(ConditionAtom::Compare(ConditionComparison {
            subject: ConditionPlayerSubject::Controller,
            left: ConditionNumber::Player(PlayerStateMeasure::ControlledPermanents {
                predicate: ObjectPredicate::IsCommander,
                exclude_source: false,
            }),
            operator: NumericComparisonOperator::GreaterThanOrEqual,
            right: ConditionNumber::Integer(1),
        }))
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn legendary_creature_presence_is_a_distinct_composed_predicate() {
    let tokens = recognize_entities("If you control a legendary creature,");
    let mut cursor = OracleTokenCursor::new("test.controlled-permanent-presence", &tokens);

    let condition = recognize_controlled_permanent_presence_condition(&mut cursor)
        .expect("legendary creature presence condition parses");
    let (predicate, _, threshold, _) =
        controlled_permanent_count(&condition).expect("presence is a controlled count");

    assert_eq!(threshold, 1);
    assert_ne!(*predicate, ObjectPredicate::IsCommander);
    assert!(predicate.guarantees_card_type(ObjectCardType::Creature));
}

#[test]
fn a_family_envelope_refuses_an_atom_outside_its_family() {
    // Near miss: a supported atom of another family is not a presence.
    let tokens = recognize_entities("If it's not your turn,");
    let mut cursor = OracleTokenCursor::new("test.controlled-permanent-presence", &tokens);
    let failure = recognize_controlled_permanent_presence_condition(&mut cursor)
        .expect_err("turn ownership is not a controlled permanent presence");
    assert_eq!(failure.state, "condition.allow-list");
}
