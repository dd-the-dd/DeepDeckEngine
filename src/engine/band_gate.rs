//! Threshold bands (level symbols CR 711.2, station symbols CR 722.2, Case
//! `Solved` CR 702.169) gate the abilities printed inside them. The parser
//! lowers each band body as an ordinary rule carrying a typed `bandGate`;
//! this module is the one runtime reader of that gate.
//!
//! A gate is honoured only where the runtime consults rule metadata for the
//! source (`rule_metadata_active_for_source`): battlefield statics and
//! keywords, triggered abilities and battlefield activated abilities. Any
//! other rule kind, and any gate kind without a runtime producer (the solved
//! designation), is reported unsupported instead of running ungated.
use serde_json::Value;

use super::{CardInstance, integer_value, value_kind};

fn counter_range(gate: &Value) -> Option<(&str, i32, Option<i32>)> {
    let counter = gate["counter"].as_str().filter(|name| !name.is_empty())?;
    let minimum = integer_value(&gate["minimum"]).filter(|minimum| *minimum >= 0)?;
    let maximum = match gate.get("maximum") {
        None => None,
        Some(maximum) => Some(integer_value(maximum).filter(|maximum| *maximum >= minimum)?),
    };
    Some((counter, minimum, maximum))
}

/// Whether the gate is well formed and the rule kind reads it at runtime.
/// A rule without a gate is unaffected.
pub(super) fn band_gate_supported(rule: &Value) -> bool {
    let Some(gate) = rule.get("bandGate") else {
        return true;
    };
    let gate_supported = match value_kind(gate) {
        Some("counterRange") => counter_range(gate).is_some(),
        // No runtime action solves a Case yet; the gate would never open,
        // and the ability would silently never apply.
        _ => false,
    };
    let kind_reads_gate = match value_kind(rule) {
        Some("staticAbility" | "keywordAbility" | "keywordAbilityGroup" | "triggeredAbility") => {
            true
        }
        // Only battlefield activations consult the source's rule metadata.
        Some("activatedAbility") => rule
            .get("activationZone")
            .is_none_or(|zone| zone.as_str() == Some("battlefield")),
        _ => false,
    };
    gate_supported && kind_reads_gate
}

/// Whether the band containing this rule is currently open for `source`
/// (CR 711.2a-b, 722.2a). A rule without a gate is always open.
pub(super) fn band_gate_open(source: &CardInstance, rule: &Value) -> bool {
    let Some(gate) = rule.get("bandGate") else {
        return true;
    };
    match value_kind(gate) {
        Some("counterRange") => {
            let Some((counter, minimum, maximum)) = counter_range(gate) else {
                return false;
            };
            let count = source.counters.get(counter).copied().unwrap_or(0);
            count >= minimum && maximum.is_none_or(|maximum| count <= maximum)
        }
        _ => false,
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "band_gate_tests.rs"]
mod tests;
