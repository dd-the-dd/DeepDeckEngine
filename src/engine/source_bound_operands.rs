//! Comparison operands of a target filter that read the ability source
//! (`with power less than this creature's power`).
//!
//! The candidate filter is a property of each candidate; its threshold is a
//! characteristic of the source, read when targets are chosen (CR 601.2c) and
//! again when the target is rechecked on resolution (CR 608.2b). The source
//! value is bound into the filter before enumeration, so every candidate
//! matcher keeps comparing a candidate property with an integer.

use serde_json::Value;

use super::{CardInstance, GameEngine, value_kind};

/// `powerOf` / `toughnessOf` the ability source.
fn source_operand(expression: &Value) -> bool {
    matches!(value_kind(expression), Some("powerOf" | "toughnessOf"))
        && matches!(
            value_kind(&expression["object"]),
            Some("self" | "abilitySource")
        )
}

/// A source-bound operand outside a target-candidate filter has no binder:
/// support checks refuse it instead of comparing against a missing value.
pub(super) fn contains_source_operand(filter: &Value) -> bool {
    match value_kind(filter) {
        Some("compare") => source_operand(&filter["right"]),
        Some("and" | "or") => filter["operands"]
            .as_array()
            .is_some_and(|operands| operands.iter().any(contains_source_operand)),
        Some("not") => contains_source_operand(&filter["operand"]),
        _ => false,
    }
}

/// Replaces each source-bound comparison operand of `filter` with the value
/// `value_of` reads. `None` when a value cannot be read.
fn bind_filter(filter: &mut Value, value_of: &mut impl FnMut(&Value) -> Option<i32>) -> Option<()> {
    match value_kind(filter) {
        Some("compare") if source_operand(&filter["right"]) => {
            let value = value_of(&filter["right"])?;
            filter["right"] = Value::from(value);
        }
        Some("and" | "or") => {
            for operand in filter["operands"].as_array_mut()? {
                bind_filter(operand, value_of)?;
            }
        }
        Some("not") => bind_filter(&mut filter["operand"], value_of)?,
        _ => {}
    }
    Some(())
}

/// Binds the source operands of a candidate domain (`permanents`, `cards`,
/// and each set of a `union`).
pub(super) fn bind_candidates(
    candidates: &mut Value,
    value_of: &mut impl FnMut(&Value) -> Option<i32>,
) -> Option<()> {
    if value_kind(candidates) == Some("union") {
        for set in candidates["sets"].as_array_mut()? {
            bind_candidates(set, value_of)?;
        }
        return Some(());
    }
    if let Some(filter) = candidates.get_mut("where") {
        bind_filter(filter, value_of)?;
    }
    Some(())
}

/// The candidate domain as a support check sees it: a source operand is a
/// well-formed integer once bound.
pub(super) fn with_placeholder_bindings(candidates: &Value) -> Value {
    let mut bound = candidates.clone();
    let _ = bind_candidates(&mut bound, &mut |_| Some(0));
    bound
}

impl GameEngine {
    /// The source's current characteristic while it is on the battlefield,
    /// else its last known one (the copy the ability carries, CR 608.2h).
    pub(super) fn source_operand_value(
        &self,
        expression: &Value,
        source: &CardInstance,
    ) -> Option<i32> {
        let source = self
            .permanent_position(&source.instance_id)
            .map(|(player, index)| &self.state.players[player].battlefield[index])
            .unwrap_or(source);
        match value_kind(expression) {
            Some("powerOf") => Some(self.current_power(source)),
            Some("toughnessOf") => Some(self.current_toughness(source)),
            _ => None,
        }
    }

    /// Binds the source operands of a candidate domain to `source`.
    pub(super) fn bind_source_candidates(
        &self,
        candidates: &mut Value,
        source: &CardInstance,
    ) -> Option<()> {
        bind_candidates(candidates, &mut |expression| {
            self.source_operand_value(expression, source)
        })
    }
}
