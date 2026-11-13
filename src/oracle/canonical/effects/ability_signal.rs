use serde_json::{Value, json};

use crate::oracle::ability_catalog::{AbilitySignalPolicy, lookup_registry};

/// Lowers only catalog registrations that explicitly publish a completion
/// signal. Documentation text and token-grammar strings are never executed.
pub(in crate::oracle::canonical) fn lower_registered_ability_signal(
    canonical_id: &str,
    player: Value,
) -> Option<Value> {
    let registration = lookup_registry(canonical_id)?;
    if registration.semantics.signal.policy != AbilitySignalPolicy::AfterActionCompletes {
        return None;
    }
    let ability_id = registration.semantics.signal.action_id.as_deref()?;
    Some(json!({
        "kind": "emitAbilitySignal",
        "abilityId": ability_id,
        "player": player,
    }))
}

#[cfg(test)]
#[path = "ability_signal_tests.rs"]
mod tests;
