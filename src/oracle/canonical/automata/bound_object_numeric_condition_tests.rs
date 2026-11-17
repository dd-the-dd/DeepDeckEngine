use crate::oracle::syntax::recognize_entities;

use super::super::{
    BoundObjectConditionNode, NumericAtom, NumericComparisonOperator, ObjectPredicate,
    SelectorNumericProperty, TargetBinding,
};
use super::{OracleTokenCursor, recognize_bound_object_numeric_condition};

fn binding() -> TargetBinding {
    TargetBinding {
        effect_index: 0,
        slot_index: 0,
    }
}

#[test]
fn mana_value_condition_retains_binding_amount_and_direction() {
    for (text, expected_operator, expected_amount) in [
        (
            "if it has mana value 2 or less",
            NumericComparisonOperator::LessThanOrEqual,
            NumericAtom::Integer(2),
        ),
        (
            "if it has mana value X or greater",
            NumericComparisonOperator::GreaterThanOrEqual,
            NumericAtom::VariableX,
        ),
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("bound-object-numeric-test", &tokens);
        assert_eq!(
            recognize_bound_object_numeric_condition(&mut cursor, binding())
                .expect("bound numeric condition parses"),
            BoundObjectConditionNode {
                object: binding(),
                predicate: ObjectPredicate::NumericComparison {
                    left: SelectorNumericProperty::ManaValue,
                    operator: expected_operator,
                    right: Box::new(expected_amount.into()),
                },
            }
        );
        cursor.finish().expect("condition consumes every token");
    }
}

#[test]
fn unsupported_direction_is_a_precise_committed_failure() {
    let tokens = recognize_entities("if it has mana value 2 or lower");
    let mut cursor = OracleTokenCursor::new("bound-object-numeric-test", &tokens);
    let failure = recognize_bound_object_numeric_condition(&mut cursor, binding())
        .expect_err("unsupported comparison language must not be approximated");

    assert_eq!(failure.state, "condition.bound-object-numeric.direction");
    assert_eq!(failure.actual.as_deref(), Some("lower"));
    assert_eq!(failure.expected, "`less` or `greater`");
}
