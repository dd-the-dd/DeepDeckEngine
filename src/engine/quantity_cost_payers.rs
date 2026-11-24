//! Quantity slots of counter actions whose amount is not a positive
//! literal: `remove all KIND counters from OBJECT` removes the object's own
//! count of that kind (CR 122.8), on the source or on any permanent a target
//! or bound reference resolves to.
//!
//! The counter executor keeps its existing body; it asks this module for the
//! object the action names and for its evaluated quantity, so a literal count
//! and a counted expression go through one removal. Cost payers for whole-
//! hand discards, attached objects and library tops are not implemented yet.

use super::*;

/// `removeCounters` support: the source or any resolvable permanent
/// reference (a target, or an object a delayed trigger captured), with a
/// positive literal count or the shared numeric expression (`counterCountOf`
/// the same object for `remove all KIND counters`).
pub(super) fn counter_removal_supported(effect: &Value) -> bool {
    let source = matches!(
        value_kind(&effect["permanent"]),
        Some("self" | "abilitySource")
    );
    let literal = integer_value(&effect["count"]).is_some_and(|count| count > 0);
    let counted =
        integer_value(&effect["count"]).is_none() && numeric_expression_supported(&effect["count"]);
    effect["counter"].is_string()
        && (literal || counted)
        && (source || target_reference_supported(&effect["permanent"]))
        && effect.get("bind").is_none_or(Value::is_string)
}

impl GameEngine {
    /// The permanent a `removeCounters` effect names: the ability's own
    /// object, or the permanent a target/bound reference resolves to.
    pub(super) fn counter_removal_instance(
        &self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<String> {
        match value_kind(&effect["permanent"]) {
            Some("self" | "abilitySource") => Some(stack_object.card.instance_id.clone()),
            _ => match self.resolve_target(&effect["permanent"], stack_object, bindings) {
                Some(TargetRef::Permanent { instance_id }) => Some(instance_id),
                _ => None,
            },
        }
    }

    /// The number of counters a `removeCounters` effect removes: a literal,
    /// or the shared numeric expression evaluated before any counter moves.
    pub(super) fn counter_removal_count(
        &self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> i32 {
        integer_value(&effect["count"])
            .or_else(|| {
                self.runtime_numeric_expression(&effect["count"], stack_object, bindings, decisions)
            })
            .unwrap_or(0)
            .max(0)
    }
}
