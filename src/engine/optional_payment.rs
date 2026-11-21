use serde_json::Value;

/// Validates the two representations of the atomic `payMana` cost. A cost is
/// either a fixed Oracle mana string or a generic amount computed from one
/// numeric expression, never both.
pub(super) fn mana_cost_supported(
    cost: &Value,
    fixed_supported: impl FnOnce(&str) -> bool,
    numeric_supported: impl FnOnce(&Value) -> bool,
) -> bool {
    match (
        cost.get("manaCost").and_then(Value::as_str),
        cost.get("genericAmount").filter(|value| !value.is_null()),
    ) {
        (Some(fixed), None) => fixed_supported(fixed),
        (None, Some(amount)) => numeric_supported(amount),
        _ => false,
    }
}

/// Materializes a payable Oracle mana cost at resolution. Dynamic amounts are
/// generic mana and clamp below zero, matching other numeric payment costs.
pub(super) fn resolve_mana_cost(
    cost: &Value,
    evaluate_numeric: impl FnOnce(&Value) -> Option<i32>,
) -> Option<String> {
    match (
        cost.get("manaCost").and_then(Value::as_str),
        cost.get("genericAmount").filter(|value| !value.is_null()),
    ) {
        (Some(fixed), None) => Some(fixed.to_string()),
        (None, Some(amount)) => Some(format!("{{{}}}", evaluate_numeric(amount)?.max(0))),
        _ => None,
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "optional_payment_tests.rs"]
mod tests;
