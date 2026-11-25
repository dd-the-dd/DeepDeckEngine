//! Lowering of the shared verb operand's non-literal forms: a typed target
//! count on a declaration, and non-target selections and collections over
//! a controller population with an X-valued predicate.
use super::*;

fn toughness_at_most_x() -> ObjectPredicate {
    ObjectPredicate::Intersection(vec![
        ObjectPredicate::CardType(ObjectCardType::Creature),
        ObjectPredicate::NumericComparison {
            left: SelectorNumericProperty::Toughness,
            operator: NumericComparisonOperator::LessThanOrEqual,
            right: Box::new(NumericExpression::Atom(NumericAtom::VariableX)),
        },
    ])
}

fn return_selected(bounds: Option<TargetCount>, query: ObjectQueryNode) -> LocalEffectNode {
    LocalEffectNode::SelectedPermanents(SelectedPermanentsEffectNode {
        action: SelectedPermanentAction::ReturnToOwnerHand,
        operand: PermanentSelectionOperandNode { bounds, query },
        until: None,
    })
}

#[test]
fn a_typed_target_count_replaces_the_literal_envelope_of_a_declaration() {
    let mut decision = target_decision(
        "targetPermanent",
        json!({"kind":"permanents", "where":{"kind":"subtypeContains","value":"Mountain"}}),
        0,
        i64::from(u32::MAX),
    );
    let x = TargetCount {
        minimum: NumericExpression::Atom(NumericAtom::VariableX),
        maximum: TargetCountBound::Amount(NumericExpression::Atom(NumericAtom::VariableX)),
    };
    apply_target_count_to_decision(&mut decision, &x).expect("X bounds lower");
    assert_eq!(decision["minimum"], decision_result("xValue"));
    assert_eq!(decision["maximum"], decision_result("xValue"));

    let any_number = TargetCount {
        minimum: NumericExpression::from(0),
        maximum: TargetCountBound::Unbounded,
    };
    apply_target_count_to_decision(&mut decision, &any_number).expect("unbounded lowers");
    assert_eq!(decision["minimum"], json!(0));
    assert_eq!(
        decision["maximum"],
        json!({"kind":"countPermanents", "allPlayers":true})
    );

    // No counted population for a stack domain: refused, never a sentinel.
    let mut spells = target_decision("targetSpell", json!({"kind":"spells"}), 0, 1);
    assert!(apply_target_count_to_decision(&mut spells, &any_number).is_none());
}

#[test]
fn a_chosen_permanent_keeps_its_source_exclusion_and_single_controller() {
    let effect = return_selected(
        Some(TargetCount::from_literal(TargetCardinality::EXACTLY_ONE)),
        ObjectQueryNode {
            predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
            controller: Some(ObjectControllerScope::AbilityController),
            exclude_source: true,
        },
    );
    let lowered = lower_targetless_local_effects(&effect, TargetlessDeclarationSupport::None, None)
        .expect("a chosen permanent returns");
    assert_eq!(lowered[0]["kind"], "choosePermanents");
    assert_eq!(lowered[0]["candidates"]["excludeSource"], json!(true));
    assert_eq!(lowered[0]["candidates"]["controller"], controller());
    assert_eq!(lowered[1]["kind"], "returnToOwnersHand");

    // A population of controllers has no single-controller candidate writer.
    let population = return_selected(
        Some(TargetCount::from_literal(TargetCardinality::EXACTLY_ONE)),
        ObjectQueryNode {
            predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
            controller: Some(ObjectControllerScope::OpponentOfAbilityController),
            exclude_source: false,
        },
    );
    assert_eq!(
        lower_targetless_local_effects(&population, TargetlessDeclarationSupport::None, None)
            .unwrap_err(),
        TargetlessLoweringError::UnsupportedCollectionController
    );
}

#[test]
fn a_collection_reads_an_opponent_population_and_requires_a_declared_x() {
    let effect = return_selected(
        None,
        ObjectQueryNode {
            predicate: toughness_at_most_x(),
            controller: Some(ObjectControllerScope::OpponentOfAbilityController),
            exclude_source: false,
        },
    );
    // Without an X producer in this context the predicate is refused.
    assert!(
        lower_targetless_local_effects(&effect, TargetlessDeclarationSupport::None, None).is_err()
    );
    let lowered =
        lower_targetless_local_effects(&effect, TargetlessDeclarationSupport::XValue, None)
            .expect("X is validated after card composition");
    assert_eq!(lowered[0]["kind"], "returnToOwnersHand");
    assert_eq!(lowered[0]["object"]["kind"], "eachPermanent");
    assert_eq!(
        lowered[0]["object"]["player"],
        json!({"kind":"opponentsOf", "player":controller()})
    );
}
