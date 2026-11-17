// Independent reviewer additions. Register as a dedicated child test module
// from effects/numeric.rs, alongside numeric_target_context_tests.rs.
// No author-owned staged source or manifest was changed by this review.
use super::*;
use crate::oracle::syntax::recognize_entities;

fn original_binding() -> TargetBinding {
    TargetBinding {
        effect_index: 0,
        slot_index: 0,
    }
}

fn original_slot(predicate: ObjectPredicate, moved: bool) -> NumericTargetSlot {
    NumericTargetSlot {
        binding: original_binding(),
        target_id: "originalTarget".into(),
        selector: TargetSelector::Object(predicate),
        requires_post_zone_change_lki: moved,
    }
}

#[test]
fn historical_its_and_that_card_carriers_cannot_bypass_lki_cap_inside_arithmetic() {
    // These are the two AST representations emitted by the shared reader:
    // `its mana value` retains ManaValueOf; `that card's mana value` now retains
    // QualifiedTargetProperty(Card). Both must cap before emitting target IR.
    let old_its = NumericExpression::ManaValueOf(original_binding());
    let old_card = NumericExpression::QualifiedTargetProperty {
        object: original_binding(),
        required_nominal: ObjectPredicate::ObjectKind(ObjectKind::Card),
        property: SelectorNumericProperty::ManaValue,
    };
    for expression in [old_its, old_card] {
        let amount = NumericExpression::Add {
            left: Box::new(NumericExpression::Atom(NumericAtom::Integer(1))),
            right: Box::new(expression),
        };
        assert_eq!(
            lower_numeric_expression_for_targets(
                &amount,
                &[original_slot(
                    ObjectPredicate::ObjectKind(ObjectKind::Card),
                    true
                )],
            ),
            Err(TargetNumericLoweringError::PostZoneChangeLki)
        );
    }
}

#[test]
fn original_its_property_is_proved_but_card_nominal_cannot_rename_a_permanent_slot() {
    let text = "Destroy target creature. You lose life equal to its mana value.";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("review-source-permanent-lki", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    let result = lower_local_effect_sequence(&mut cursor, &sequence).unwrap();
    assert_eq!(
        result.effects[1]["amount"],
        json!({"kind":"manaValueOf","object":{"kind":"chosenTarget","id":"targetPermanent"}})
    );
    let tokens = recognize_entities(
        "Destroy target creature. You lose life equal to that card's mana value.",
    );
    let mut cursor = OracleTokenCursor::new("review-source-card-nominal", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    assert_eq!(
        lower_local_effect_sequence(&mut cursor, &sequence)
            .err()
            .unwrap()
            .state,
        "sequence.target-numeric.nominal"
    );
}

#[test]
fn moved_stack_and_mixed_domains_remain_capped_for_both_numeric_carriers() {
    let permanent = ObjectPredicate::ObjectKind(ObjectKind::Permanent);
    let spell = ObjectPredicate::ObjectKind(ObjectKind::Spell);
    for selector in [
        TargetSelector::StackItem(StackItemPredicate::Spell),
        TargetSelector::Object(spell.clone()),
        TargetSelector::Object(ObjectPredicate::Union(vec![
            permanent.clone(),
            spell.clone(),
        ])),
        TargetSelector::Union(vec![
            TargetSelector::Object(permanent),
            TargetSelector::Object(spell.clone()),
        ]),
    ] {
        for expression in [
            NumericExpression::ManaValueOf(original_binding()),
            NumericExpression::QualifiedTargetProperty {
                object: original_binding(),
                required_nominal: spell.clone(),
                property: SelectorNumericProperty::ManaValue,
            },
        ] {
            let target = NumericTargetSlot {
                binding: original_binding(),
                target_id: "originalTarget".into(),
                selector: selector.clone(),
                requires_post_zone_change_lki: true,
            };
            let error = lower_numeric_expression_for_targets(&expression, &[target]).unwrap_err();
            assert!(matches!(
                error,
                TargetNumericLoweringError::PostZoneChangeLki
                    | TargetNumericLoweringError::NominalMismatch
            ));
        }
    }
}

#[test]
fn post_zone_permanent_lki_does_not_enable_noncreature_power_or_toughness() {
    for property in [
        SelectorNumericProperty::Power,
        SelectorNumericProperty::Toughness,
    ] {
        let expression = NumericExpression::QualifiedTargetProperty {
            object: original_binding(),
            required_nominal: ObjectPredicate::CardType(ObjectCardType::Artifact),
            property,
        };
        assert_eq!(
            lower_numeric_expression_for_targets(
                &expression,
                &[original_slot(
                    ObjectPredicate::CardType(ObjectCardType::Artifact),
                    true
                )],
            ),
            Err(TargetNumericLoweringError::UnprovedCharacteristicDomain)
        );
    }
}

#[test]
fn numeric_binding_must_match_both_effect_and_slot_ordinals() {
    let expression = NumericExpression::ManaValueOf(original_binding());
    for binding in [
        TargetBinding {
            effect_index: 1,
            slot_index: 0,
        },
        TargetBinding {
            effect_index: 0,
            slot_index: 1,
        },
    ] {
        let mut slot = original_slot(ObjectPredicate::CardType(ObjectCardType::Creature), false);
        slot.binding = binding;
        assert_eq!(
            lower_numeric_expression_for_targets(&expression, &[slot]),
            Err(TargetNumericLoweringError::MissingBinding)
        );
    }
}

#[test]
fn a_later_artifact_target_cannot_be_renamed_to_an_earlier_creature() {
    let text = "Target creature gets +1/+1 until end of turn. Target artifact gets +2/+2 until end of turn. You gain life equal to that creature's power.";
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("review-source-original-nominal", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    let failure = lower_local_effect_sequence(&mut cursor, &sequence)
        .err()
        .expect("the nominal cannot redirect an anaphor to a previous slot");
    assert_eq!(failure.state, "sequence.target-numeric.nominal");
}

#[test]
fn unproved_noncreature_power_cannot_enter_the_new_contextual_lowerer() {
    // Regression for CR 208.3/208.3a: stored +2/+2 on a noncreature is dormant.
    // The current runtime current_power still adds that modifier. Until proved,
    // both old `its power` and new named-nominal carriers must remain capped.
    for expression in [
        NumericExpression::PowerOf(original_binding()),
        NumericExpression::QualifiedTargetProperty {
            object: original_binding(),
            required_nominal: ObjectPredicate::CardType(ObjectCardType::Artifact),
            property: SelectorNumericProperty::Power,
        },
        NumericExpression::QualifiedTargetProperty {
            object: original_binding(),
            required_nominal: ObjectPredicate::CardType(ObjectCardType::Artifact),
            property: SelectorNumericProperty::Toughness,
        },
    ] {
        assert!(
            lower_numeric_expression_for_targets(
                &expression,
                &[original_slot(
                    ObjectPredicate::CardType(ObjectCardType::Artifact),
                    false
                ),]
            )
            .is_err()
        );
    }
    // Mana value remains defined for the same artifact; keep that capability.
    assert!(
        lower_numeric_expression_for_targets(
            &NumericExpression::ManaValueOf(original_binding()),
            &[original_slot(
                ObjectPredicate::CardType(ObjectCardType::Artifact),
                false
            )],
        )
        .is_ok()
    );
}

#[test]
fn historical_card_possessive_retains_case_insensitive_suffix_recognition() {
    for text in ["equal to THAT CARD'S POWER", "equal to THAT CARD’S POWER"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("review-source-possessive-case", &tokens);
        let expression = recognize_numeric_expression(&mut cursor, Some(original_binding()))
            .expect(
                "case-insensitive lexical recognition must preserve old card possessive support",
            );
        assert!(
            matches!(expression, NumericExpression::QualifiedTargetProperty {
            object, required_nominal: ObjectPredicate::ObjectKind(ObjectKind::Card),
            property: SelectorNumericProperty::Power,
        } if object == original_binding())
        );
        cursor.finish().unwrap();
    }
}
