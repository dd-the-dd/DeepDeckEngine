use super::super::*;

pub(in crate::oracle::canonical) fn numeric_atom_uses_x(atom: &NumericAtom) -> bool {
    atom == &NumericAtom::VariableX
}

pub(in crate::oracle::canonical) fn lower_numeric_atom(atom: NumericAtom) -> Value {
    match atom {
        NumericAtom::Integer(value) => integer(value),
        NumericAtom::VariableX => decision_result("xValue"),
    }
}

/// Target declarations historically encode literal bounds as scalar JSON,
/// while `X` remains a casting decision result. Keep that stable shape while
/// allowing the recognizer to retain one typed bound representation.
pub(in crate::oracle::canonical) fn lower_target_cardinality_bound(atom: NumericAtom) -> Value {
    match atom {
        NumericAtom::Integer(value) => json!(value),
        NumericAtom::VariableX => decision_result("xValue"),
    }
}

/// Lowers expressions that do not require a target-property binding. The
/// caller retains ownership of binding-aware forms such as `PowerOf`.
pub(in crate::oracle::canonical) fn lower_numeric_expression(
    expression: &NumericExpression,
) -> Option<Value> {
    match expression {
        NumericExpression::Atom(atom) => Some(lower_numeric_atom(atom.clone())),
        NumericExpression::ObjectProperty {
            object: LocalObjectReference::SourceSelf,
            property,
        } => Some(json!({
            "kind": match property {
                NumericObjectProperty::Power => "powerOf",
                NumericObjectProperty::Toughness => "toughnessOf",
            },
            "object": self_ref(),
        })),
        NumericExpression::SourceCastX => Some(json!({ "kind": "sourceCastXValue" })),
        NumericExpression::EventValue(EventNumericBinding::DamageAmount) => {
            Some(decision_result("damageAmount"))
        }
        NumericExpression::CountDistinctCardTypes { zone } => Some(json!({
            "kind": "countDistinctCardTypes",
            "zone": match zone {
                GraveyardScope::Controller => graveyard(controller()),
                GraveyardScope::AllPlayers => json!({ "kind": "anyGraveyard" }),
            },
        })),
        NumericExpression::Add { left, right } => Some(json!({
            "kind": "add",
            "left": lower_numeric_expression(left)?,
            "right": lower_numeric_expression(right)?,
        })),
        NumericExpression::Subtract { left, right } => Some(json!({
            "kind": "subtract",
            "left": lower_numeric_expression(left)?,
            "right": lower_numeric_expression(right)?,
        })),
        NumericExpression::Multiply { left, right } => Some(json!({
            "kind": "multiply",
            "left": lower_numeric_expression(left)?,
            "right": lower_numeric_expression(right)?,
        })),
        NumericExpression::CardsInZone {
            player: LocalPlayerReference::Controller,
            zone: LocalZone::Library,
        } => Some(json!({
            "kind": "countCards",
            "zone": { "kind": "library", "player": controller() },
            "where": Value::Null,
        })),
        NumericExpression::CardsInZone { .. } => None,
        NumericExpression::Divide {
            left,
            right,
            rounding,
        } => Some(json!({
            "kind": "divide",
            "left": lower_numeric_expression(left)?,
            "right": lower_numeric_expression(right)?,
            "round": match rounding {
                NumericRounding::Down => "down",
                NumericRounding::Up => "up",
            },
        })),
        NumericExpression::PowerOf(_) | NumericExpression::ManaValueOf(_) => None,
    }
}

pub(in crate::oracle::canonical) fn lower_selector_numeric_comparison(
    left: SelectorNumericProperty,
    operator: NumericComparisonOperator,
    right: &NumericAtom,
) -> Value {
    let left = match left {
        SelectorNumericProperty::ManaValue => json!({
            "kind": "manaValueOf",
            "object": { "kind": "candidate" },
        }),
        SelectorNumericProperty::Power => json!({
            "kind": "powerOf",
            "object": { "kind": "candidate" },
        }),
        SelectorNumericProperty::Toughness => json!({
            "kind": "toughnessOf",
            "object": { "kind": "candidate" },
        }),
    };
    let operator = match operator {
        NumericComparisonOperator::Equal => "==",
        NumericComparisonOperator::LessThanOrEqual => "<=",
        NumericComparisonOperator::GreaterThanOrEqual => ">=",
    };
    compare(operator, left, lower_numeric_atom(right.clone()))
}

#[cfg(test)]
#[path = "numeric_tests.rs"]
mod tests;
