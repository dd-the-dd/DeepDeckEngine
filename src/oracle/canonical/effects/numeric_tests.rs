use super::*;

#[test]
fn source_cast_x_lowers_to_its_existing_runtime_expression() {
    assert_eq!(
        lower_numeric_expression(&NumericExpression::SourceCastX),
        Some(json!({ "kind": "sourceCastXValue" }))
    );
}

#[test]
fn source_object_properties_lower_to_existing_runtime_expressions() {
    for (property, kind) in [
        (NumericObjectProperty::Power, "powerOf"),
        (NumericObjectProperty::Toughness, "toughnessOf"),
    ] {
        assert_eq!(
            lower_numeric_expression(&NumericExpression::ObjectProperty {
                object: LocalObjectReference::SourceSelf,
                property,
            }),
            Some(json!({
                "kind": kind,
                "object": { "kind": "self" },
            }))
        );
    }
}

#[test]
fn selector_properties_lower_to_candidate_runtime_expressions() {
    for (property, kind) in [
        (SelectorNumericProperty::ManaValue, "manaValueOf"),
        (SelectorNumericProperty::Power, "powerOf"),
        (SelectorNumericProperty::Toughness, "toughnessOf"),
    ] {
        assert_eq!(
            lower_selector_numeric_comparison(
                property,
                NumericComparisonOperator::LessThanOrEqual,
                &NumericExpression::from(2),
            )
            .expect("literal comparison lowers"),
            json!({
                "kind": "compare",
                "operator": "<=",
                "left": { "kind": kind, "object": { "kind": "candidate" } },
                "right": { "kind": "integer", "value": 2 },
            })
        );
    }
}

#[test]
fn rounded_down_literal_division_lowers_recursively() {
    let expression = NumericExpression::divide_rounded_down(NumericExpression::SourceCastX, 2)
        .expect("positive literal divisor");

    assert_eq!(
        lower_numeric_expression(&expression),
        Some(json!({
            "kind": "divide",
            "left": { "kind": "sourceCastXValue" },
            "right": { "kind": "integer", "value": 2 },
            "round": "down",
        }))
    );
}

#[test]
fn arithmetic_operations_lower_to_existing_recursive_runtime_primitives() {
    let expression = NumericExpression::divide(
        NumericExpression::multiply(
            NumericExpression::add(
                NumericExpression::Atom(NumericAtom::VariableX),
                NumericExpression::Atom(NumericAtom::Integer(2)),
            ),
            NumericExpression::Atom(NumericAtom::Integer(3)),
        ),
        NumericExpression::subtract(
            NumericExpression::Atom(NumericAtom::Integer(5)),
            NumericExpression::Atom(NumericAtom::Integer(3)),
        ),
        NumericRounding::Up,
    )
    .expect("the divisor is a nonzero expression");

    assert_eq!(
        lower_numeric_expression(&expression),
        Some(json!({
            "kind": "divide",
            "left": {
                "kind": "multiply",
                "left": {
                    "kind": "add",
                    "left": { "kind": "decisionResult", "decisionId": "xValue" },
                    "right": { "kind": "integer", "value": 2 },
                },
                "right": { "kind": "integer", "value": 3 },
            },
            "right": {
                "kind": "subtract",
                "left": { "kind": "integer", "value": 5 },
                "right": { "kind": "integer", "value": 3 },
            },
            "round": "up",
        }))
    );
}

#[test]
fn rounded_down_division_rejects_nonpositive_divisors() {
    for divisor in [0, -1] {
        assert_eq!(
            NumericExpression::divide_rounded_down(NumericExpression::SourceCastX, divisor),
            None
        );
    }
}

#[test]
fn target_property_expression_requires_a_binding_aware_lowerer() {
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    assert_eq!(
        lower_numeric_expression(&NumericExpression::PowerOf(binding)),
        None
    );
}

#[test]
fn rounded_controller_library_count_lowers_to_shared_count_and_divide_ir() {
    let expression = NumericExpression::divide(
        NumericExpression::CardsInZone {
            player: LocalPlayerReference::Controller,
            zone: LocalZone::Library,
        },
        2,
        NumericRounding::Up,
    )
    .expect("positive divisor");

    assert_eq!(
        lower_numeric_expression(&expression),
        Some(json!({
            "kind": "divide",
            "left": {
                "kind": "countCards",
                "zone": { "kind": "library", "player": controller() },
                "where": Value::Null,
            },
            "right": integer(2),
            "round": "up",
        }))
    );
}
