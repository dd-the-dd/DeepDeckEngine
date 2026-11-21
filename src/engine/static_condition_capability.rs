//! Shared capability proof for characteristic-independent static conditions.
use super::*;

/// Capability check only; the established shared condition evaluator executes it.
/// A genuine P/T-dependent color rule needs a layered characteristic evaluator.
pub(super) fn condition_is_closed_without_characteristic_reads(
    condition: &Value,
    depth: usize,
) -> bool {
    if depth >= 64 {
        return false;
    }
    match value_kind(condition) {
        Some("compare") => {
            integer_value(&condition["left"]).is_some()
                && integer_value(&condition["right"]).is_some()
                && condition_supported(condition)
        }
        Some("sourceOnBattlefield" | "sourceFlagSet") => condition_supported(condition),
        Some("and" | "or") => condition["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty()
                && operands.iter().all(|operand| {
                    condition_is_closed_without_characteristic_reads(operand, depth + 1)
                })
        }),
        Some("not") => {
            condition_is_closed_without_characteristic_reads(&condition["operand"], depth + 1)
        }
        _ => false,
    }
}
