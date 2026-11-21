use crate::oracle::syntax::recognize_entities;

use super::super::{BoundObjectConditionNode, CardColor, ObjectPredicate, TargetBinding};
use super::{OracleTokenCursor, recognize_bound_object_color_condition};

#[test]
fn color_condition_retains_the_parent_object_binding() {
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    let tokens = recognize_entities("If it's a green card,");
    let mut cursor = OracleTokenCursor::new("bound-object-color-test", &tokens);

    assert_eq!(
        recognize_bound_object_color_condition(&mut cursor, binding)
            .expect("bound color condition parses"),
        BoundObjectConditionNode {
            object: binding,
            predicate: ObjectPredicate::Color(CardColor::Green),
        }
    );
}

#[test]
fn unsupported_characteristic_fails_without_losing_the_binding() {
    let tokens = recognize_entities("If it's a striped card,");
    let mut cursor = OracleTokenCursor::new("bound-object-color-test", &tokens);

    let failure = recognize_bound_object_color_condition(
        &mut cursor,
        TargetBinding {
            effect_index: 2,
            slot_index: 1,
        },
    )
    .expect_err("unknown characteristics must not be approximated as colors");

    assert_eq!(failure.actual.as_deref(), Some("striped"));
    assert_eq!(failure.expected, "a Magic color");
}
