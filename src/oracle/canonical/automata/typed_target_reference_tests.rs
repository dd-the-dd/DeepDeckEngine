use super::super::{
    LocalPlayerReference, NumericExpression, SelectorNumericProperty, recognize_numeric_expression,
    recognize_player_reference, recognize_player_reference_with_support,
};
use super::*;
use crate::oracle::syntax::recognize_entities;

fn binding() -> TargetBinding {
    TargetBinding {
        effect_index: 0,
        slot_index: 0,
    }
}

#[test]
fn numeric_and_controller_references_share_the_registered_possessive_nominal() {
    for surface in ["that creature's", "THAT CREATURE'S", "that creature’s"] {
        let text = format!("{surface} controller creates");
        let tokens = recognize_entities(&text);
        let mut cursor = OracleTokenCursor::new("typed-controller-reference", &tokens);
        let reference = recognize_player_reference_with_support(
            &mut cursor,
            Some(binding()),
            TargetReferenceSupport::TypedNominal,
        )
        .unwrap();
        assert_eq!(
            reference,
            LocalPlayerReference::ControllerOfQualified(TypedTargetReference {
                binding: binding(),
                required_nominal: Some(TargetNominal::CardType(ObjectCardType::Creature)),
            })
        );
        assert_eq!(cursor.peek(), Some("creates"));
        let text = format!("equal to {surface} power");
        let tokens = recognize_entities(&text);
        let mut cursor = OracleTokenCursor::new("typed-numeric-reference", &tokens);
        assert_eq!(
            recognize_numeric_expression(&mut cursor, Some(binding())).unwrap(),
            NumericExpression::QualifiedTargetProperty {
                object: binding(),
                required_nominal: ObjectPredicate::CardType(ObjectCardType::Creature),
                property: SelectorNumericProperty::Power,
            }
        );
    }
}

#[test]
fn historical_its_reference_retains_unqualified_identity_and_missing_binding_fails() {
    let tokens = recognize_entities("its controller");
    let mut cursor = OracleTokenCursor::new("old-controller-reference", &tokens);
    assert_eq!(
        recognize_player_reference(&mut cursor, Some(binding())).unwrap(),
        LocalPlayerReference::ControllerOf(binding())
    );
    let tokens = recognize_entities("that creature's controller");
    let mut cursor = OracleTokenCursor::new("missing-controller-binding", &tokens);
    assert!(
        recognize_player_reference_with_support(
            &mut cursor,
            None,
            TargetReferenceSupport::TypedNominal,
        )
        .is_err()
    );
}

#[test]
fn nominal_evidence_must_hold_for_every_original_target_alternative() {
    let creature = ObjectPredicate::CardType(ObjectCardType::Creature);
    let artifact = ObjectPredicate::CardType(ObjectCardType::Artifact);
    let compound = TargetSelector::Object(ObjectPredicate::Intersection(vec![
        creature.clone(),
        artifact.clone(),
    ]));
    assert!(selector_implies_nominal(&compound, &creature));
    let either = TargetSelector::Object(ObjectPredicate::Union(vec![creature.clone(), artifact]));
    assert!(!selector_implies_nominal(&either, &creature));
    assert!(selector_implies_nominal(
        &either,
        &ObjectPredicate::ObjectKind(ObjectKind::Permanent)
    ));
}

#[test]
fn unqualified_wrappers_reject_nominal_actor_before_producing_a_new_variant() {
    let tokens = recognize_entities("that creature's controller gains 1 life");
    let mut cursor = OracleTokenCursor::new("old-actor-reference-cap", &tokens);
    assert!(recognize_player_reference(&mut cursor, Some(binding())).is_err());
    let tokens =
        recognize_entities("that creature's controller creates a 1/1 white Soldier creature token");
    let mut cursor = OracleTokenCursor::new("old-token-reference-cap", &tokens);
    assert!(
        super::super::recognize_create_creature_tokens_effect(&mut cursor, Some(binding()))
            .is_err()
    );
}
