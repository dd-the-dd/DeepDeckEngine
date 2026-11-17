use super::*;

fn binding() -> TargetBinding {
    TargetBinding {
        effect_index: 0,
        slot_index: 0,
    }
}
fn expression() -> NumericExpression {
    NumericExpression::QualifiedTargetProperty {
        object: binding(),
        required_nominal: ObjectPredicate::CardType(ObjectCardType::Creature),
        property: SelectorNumericProperty::Power,
    }
}
fn slot(predicate: ObjectPredicate, moved: bool) -> NumericTargetSlot {
    NumericTargetSlot {
        binding: binding(),
        target_id: "theOriginalTarget".into(),
        selector: TargetSelector::Object(predicate),
        requires_post_zone_change_lki: moved,
    }
}

#[test]
fn contextual_arithmetic_reuses_one_original_target_without_changing_default_caps() {
    let amount = NumericExpression::Add {
        left: Box::new(NumericExpression::Atom(NumericAtom::Integer(2))),
        right: Box::new(expression()),
    };
    assert_eq!(lower_numeric_expression(&amount), None);
    let slots = [slot(
        ObjectPredicate::CardType(ObjectCardType::Creature),
        false,
    )];
    assert_eq!(
        lower_numeric_expression_for_targets(&amount, &slots).unwrap(),
        json!({"kind":"add","left":{"kind":"integer","value":2},"right":{"kind":"powerOf","object":{"kind":"chosenTarget","id":"theOriginalTarget"}}})
    );
}

#[test]
fn required_nominal_must_hold_for_every_target_alternative() {
    for predicate in [
        ObjectPredicate::CardType(ObjectCardType::Artifact),
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Creature),
            ObjectPredicate::CardType(ObjectCardType::Enchantment),
        ]),
        ObjectPredicate::Not(Box::new(ObjectPredicate::CardType(
            ObjectCardType::Creature,
        ))),
    ] {
        assert_eq!(
            lower_numeric_expression_for_targets(&expression(), &[slot(predicate, false)]),
            Err(TargetNumericLoweringError::NominalMismatch)
        );
    }
    let compound = ObjectPredicate::Intersection(vec![
        ObjectPredicate::CardType(ObjectCardType::Artifact),
        ObjectPredicate::CardType(ObjectCardType::Creature),
    ]);
    assert!(lower_numeric_expression_for_targets(&expression(), &[slot(compound, false)]).is_ok());
}

#[test]
fn moved_permanent_target_keeps_original_identity_and_absent_binding_stays_rejected() {
    assert_eq!(
        lower_numeric_expression_for_targets(&expression(), &[]),
        Err(TargetNumericLoweringError::MissingBinding)
    );
    assert_eq!(
        lower_numeric_expression_for_targets(
            &expression(),
            &[slot(
                ObjectPredicate::CardType(ObjectCardType::Creature),
                true
            )]
        ),
        Ok(json!({"kind":"powerOf","object":{"kind":"chosenTarget","id":"theOriginalTarget"}}))
    );
}
