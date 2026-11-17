use super::super::{lower_numeric_expression, lower_targetless_numeric_expression};
use super::*;

fn literal(n: i64) -> NumericExpression {
    NumericExpression::Atom(NumericAtom::Integer(n))
}
fn lower(
    expr: &NumericExpression,
    support: TargetlessDeclarationSupport,
) -> Result<serde_json::Value, TargetlessLoweringError> {
    lower_targetless_numeric_expression(expr, support)
}

#[test]
fn valid_nested_arithmetic_keeps_its_existing_unfolded_ir() {
    let expression = NumericExpression::divide_rounded_down(
        NumericExpression::add(
            literal(1),
            NumericExpression::multiply(literal(2), literal(3)),
        ),
        2_i64,
    )
    .unwrap();
    assert_eq!(
        lower(&expression, TargetlessDeclarationSupport::None).unwrap(),
        lower_numeric_expression(&expression).unwrap()
    );
    for rounding in [NumericRounding::Down, NumericRounding::Up] {
        let expression = NumericExpression::divide(literal(-5), literal(2), rounding).unwrap();
        assert_eq!(
            lower(&expression, TargetlessDeclarationSupport::None).unwrap(),
            lower_numeric_expression(&expression).unwrap()
        );
    }
}

#[test]
fn nested_literals_and_constant_intermediates_must_fit_i32() {
    let oversized = NumericExpression::add(
        NumericExpression::SourceCastX,
        NumericExpression::multiply(literal(1), literal(i64::MAX)),
    );
    assert_eq!(
        lower(&oversized, TargetlessDeclarationSupport::None).unwrap_err(),
        TargetlessLoweringError::RuntimeIntegerRange
    );
    for expression in [
        NumericExpression::add(literal(i64::from(i32::MAX)), literal(1)),
        NumericExpression::multiply(literal(i64::from(i32::MAX)), literal(2)),
        NumericExpression::subtract(literal(i64::from(i32::MIN)), literal(1)),
    ] {
        assert_eq!(
            lower(&expression, TargetlessDeclarationSupport::None).unwrap_err(),
            TargetlessLoweringError::RuntimeArithmeticRange
        );
    }
    let zero_divisor = NumericExpression::divide(
        NumericExpression::SourceCastX,
        NumericExpression::subtract(literal(2), literal(2)),
        NumericRounding::Down,
    )
    .unwrap();
    assert_eq!(
        lower(&zero_divisor, TargetlessDeclarationSupport::None).unwrap_err(),
        TargetlessLoweringError::InvalidNumericArithmetic
    );
}

#[test]
fn nested_x_requires_the_actual_context_and_does_not_create_a_producer() {
    let expression = NumericExpression::add(
        literal(1),
        NumericExpression::multiply(literal(2), NumericExpression::Atom(NumericAtom::VariableX)),
    );
    assert!(expression.uses_x());
    assert_eq!(
        lower(&expression, TargetlessDeclarationSupport::None).unwrap_err(),
        TargetlessLoweringError::RequiresDeclaration
    );
    assert_eq!(
        lower(&expression, TargetlessDeclarationSupport::XValue).unwrap(),
        lower_numeric_expression(&expression).unwrap()
    );
}

#[test]
fn source_cast_values_and_unsupported_target_properties_keep_their_own_boundaries() {
    use super::super::{
        LocalObjectReference, LocalPlayerReference, LocalZone, NumericObjectProperty, TargetBinding,
    };
    for expression in [
        NumericExpression::SourceCastX,
        NumericExpression::ObjectProperty {
            object: LocalObjectReference::SourceSelf,
            property: NumericObjectProperty::Power,
        },
        NumericExpression::CardsInZone {
            player: LocalPlayerReference::Controller,
            zone: LocalZone::Library,
        },
    ] {
        assert!(!expression.uses_x());
        assert_eq!(
            lower(&expression, TargetlessDeclarationSupport::None).unwrap(),
            lower_numeric_expression(&expression).unwrap()
        );
    }
    let target = NumericExpression::PowerOf(TargetBinding {
        effect_index: 0,
        slot_index: 0,
    });
    assert_eq!(
        lower(&target, TargetlessDeclarationSupport::None).unwrap_err(),
        TargetlessLoweringError::RequiresTarget
    );
    let nested = NumericExpression::add(literal(1), target);
    assert_eq!(
        lower(&nested, TargetlessDeclarationSupport::None).unwrap_err(),
        TargetlessLoweringError::UnsupportedEffect
    );
}

#[test]
fn quantified_discard_keeps_actors_and_random_binding_ids_after_validation() {
    use super::super::{
        CollectionSelectionMethod, PlayerDiscardEffectNode, PlayerDiscardLoweringIds,
        PlayerEffectSubject, PlayerSetReferenceNode, QuantifiedPlayerAction,
        QuantifiedPlayerActionNode, controller, lower_player_discard_action_for_player,
        lower_quantified_player_action,
    };
    for selection in [
        CollectionSelectionMethod::Choice,
        CollectionSelectionMethod::Random,
    ] {
        let action = PlayerDiscardEffectNode {
            player: PlayerEffectSubject::Controller,
            count: NumericExpression::add(literal(1), literal(2)),
            selection,
        };
        let bound = serde_json::json!({"kind":"boundValue", "id":"quantifiedPlayer"});
        let expected_child = lower_player_discard_action_for_player(
            &action,
            PlayerDiscardLoweringIds {
                target_player: "unusedQuantifiedTarget",
                random_selection: "quantifiedDiscard",
            },
            bound.clone(),
        )
        .unwrap();
        let lowered = lower_quantified_player_action(
            &QuantifiedPlayerActionNode {
                players: PlayerSetReferenceNode::OpponentsOfAbilityController,
                excluding_controller: false,
                actions: vec![QuantifiedPlayerAction::Discard(action)],
            },
            TargetlessDeclarationSupport::None,
        )
        .unwrap();
        assert_eq!(
            lowered,
            serde_json::json!({
                "kind":"forEachPlayer", "players":{"kind":"opponentsOf", "player":controller()},
                "bind":"quantifiedPlayer", "effects":expected_child,
            })
        );
        assert_eq!(lowered["effects"][0]["player"], bound);
        if selection == CollectionSelectionMethod::Random {
            assert_eq!(lowered["effects"][0]["id"], "quantifiedDiscard");
            assert_eq!(
                lowered["effects"][1]["selectionProvenance"]["decisionId"],
                "quantifiedDiscard"
            );
        }
    }
}
