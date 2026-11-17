use super::*;

#[test]
fn runtime_literal_bounds_do_not_cap_draw_counts_at_sixty_four() {
    for value in [65_i64, i64::from(i32::MAX)] {
        assert_eq!(
            lower_targetless_numeric_expression(
                &NumericExpression::Atom(NumericAtom::Integer(value)),
                TargetlessDeclarationSupport::None,
            )
            .unwrap(),
            integer(value)
        );
    }
    assert_eq!(
        lower_targetless_numeric_expression(
            &NumericExpression::Atom(NumericAtom::Integer(i64::from(i32::MAX) + 1)),
            TargetlessDeclarationSupport::None,
        )
        .unwrap_err(),
        TargetlessLoweringError::RuntimeIntegerRange
    );
}

#[test]
fn numeric_context_retains_declared_x_and_symbolic_source_properties() {
    let variable = NumericExpression::Atom(NumericAtom::VariableX);
    assert_eq!(
        lower_targetless_numeric_expression(&variable, TargetlessDeclarationSupport::None)
            .unwrap_err(),
        TargetlessLoweringError::RequiresDeclaration
    );
    assert_eq!(
        lower_targetless_numeric_expression(&variable, TargetlessDeclarationSupport::XValue)
            .unwrap(),
        decision_result("xValue")
    );
    assert_eq!(
        lower_targetless_numeric_expression(
            &NumericExpression::SourceCastX,
            TargetlessDeclarationSupport::None
        )
        .unwrap()["kind"],
        "sourceCastXValue"
    );
}
