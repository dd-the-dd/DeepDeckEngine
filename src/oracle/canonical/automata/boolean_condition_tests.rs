use crate::oracle::syntax::recognize_entities;

use super::super::{
    EffectConnector, LocalConditionNode, ObjectCardType, ObjectPredicate,
    recognize_effect_sequence, recognize_object_type_union_list,
};
use super::{
    BooleanConditionNode, OracleTokenCursor, recognize_controlled_count_boolean_condition_body,
};

fn recognize_complete_boolean(text: &str) -> super::MachineResult<BooleanConditionNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("boolean-condition-test", &tokens);
    let condition = recognize_controlled_count_boolean_condition_body(&mut cursor)?;
    cursor.finish()?;
    Ok(condition)
}

#[test]
fn arbitrary_oxford_conjunction_composes_existing_condition_atoms() {
    let condition = recognize_complete_boolean(
        "you control one or more artifacts, you control two or more creatures, and you control three or more enchantments",
    )
    .expect("three condition atoms parse");

    let BooleanConditionNode::And(conditions) = condition else {
        panic!("expected one n-ary Boolean conjunction");
    };
    assert_eq!(conditions.len(), 3);
    for condition in conditions {
        assert!(matches!(
            condition,
            BooleanConditionNode::Atom(LocalConditionNode::ControlledPermanentCount(_))
        ));
    }
}

#[test]
fn oxford_disjunction_and_prefix_negation_remain_typed() {
    let condition = recognize_complete_boolean(
        "not you control one or more artifacts, or you control two or more creatures",
    )
    .expect("negated disjunction parses");

    let BooleanConditionNode::Or(conditions) = condition else {
        panic!("expected one n-ary Boolean disjunction");
    };
    assert!(matches!(conditions[0], BooleanConditionNode::Not(_)));
    assert!(matches!(conditions[1], BooleanConditionNode::Atom(_)));
}

#[test]
fn and_is_effect_order_in_an_effect_parent() {
    let tokens = recognize_entities("you gain one life and you gain two life.");
    let mut cursor = OracleTokenCursor::new("connector-context-effect-test", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)
        .expect("the effect parent recognizes ordered actions");

    assert_eq!(sequence.effects.len(), 2);
    assert_eq!(sequence.connectors, [EffectConnector::And]);
}

#[test]
fn and_is_candidate_union_in_a_type_list_parent() {
    let tokens = recognize_entities("artifact and enchantment");
    let mut cursor = OracleTokenCursor::new("connector-context-target-test", &tokens);
    let predicate = recognize_object_type_union_list(&mut cursor)
        .expect("the selector parent recognizes candidate union");
    cursor.finish().expect("the type list is consumed");

    assert_eq!(
        predicate,
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ])
    );
}

#[test]
fn and_is_boolean_conjunction_in_a_condition_parent() {
    let condition = recognize_complete_boolean(
        "you control one or more artifacts, and you control two or more creatures",
    )
    .expect("the condition parent recognizes Boolean conjunction");

    assert!(matches!(condition, BooleanConditionNode::And(values) if values.len() == 2));
}
