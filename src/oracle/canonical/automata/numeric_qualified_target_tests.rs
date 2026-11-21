use super::*;
use crate::oracle::syntax::recognize_entities;

fn binding() -> TargetBinding {
    TargetBinding {
        effect_index: 0,
        slot_index: 0,
    }
}

#[test]
fn qualified_reference_carries_its_nominal_until_parent_validation() {
    for (noun, nominal) in [
        (
            "permanent",
            super::super::ObjectPredicate::ObjectKind(super::super::ObjectKind::Permanent),
        ),
        (
            "creature",
            super::super::ObjectPredicate::CardType(super::super::ObjectCardType::Creature),
        ),
        (
            "artifact",
            super::super::ObjectPredicate::CardType(super::super::ObjectCardType::Artifact),
        ),
        (
            "card",
            super::super::ObjectPredicate::ObjectKind(super::super::ObjectKind::Card),
        ),
    ] {
        let tokens = recognize_entities(&format!("equal to that {noun}'s power"));
        let mut cursor = OracleTokenCursor::new("qualified-target-test", &tokens);
        assert_eq!(
            recognize_numeric_expression(&mut cursor, Some(binding())).unwrap(),
            NumericExpression::QualifiedTargetProperty {
                object: binding(),
                required_nominal: nominal,
                property: super::super::SelectorNumericProperty::Power
            }
        );
        cursor.finish().unwrap();
    }
}

#[test]
fn an_object_reference_requires_an_antecedent_and_a_registered_object_nominal() {
    for text in [
        "equal to that permanent's mana value",
        "equal to that player's power",
        "equal to that gadget's power",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("qualified-target-negative", &tokens);
        assert!(
            recognize_numeric_expression(&mut cursor, None).is_err(),
            "{text}"
        );
    }
    let tokens = recognize_entities("equal to that player's power");
    let mut cursor = OracleTokenCursor::new("qualified-target-player", &tokens);
    assert!(recognize_numeric_expression(&mut cursor, Some(binding())).is_err());
}
