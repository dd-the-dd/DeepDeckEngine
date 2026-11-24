use serde_json::Value;

use super::super::{
    numeric_expression_supported, object_expression_supported, player_reference_supported,
    target_reference_supported, value_kind,
};

/// Validates direct-damage IR nodes.
///
/// `None` means this validator does not own the effect kind. `Some(false)`
/// means it owns the kind but its schema is invalid.
pub(in super::super) fn damage_effect_supported(effect: &Value) -> Option<bool> {
    let supported = match value_kind(effect) {
        Some("dealDamage") => {
            numeric_expression_supported(&effect["amount"])
                && (target_reference_supported(&effect["recipient"])
                    // Each chosen target of a slot of several (CR 601.2c).
                    || (value_kind(&effect["recipient"]) == Some("chosenTargets")
                        && effect["recipient"]["id"].is_string())
                    // The entering object, resolved only while it is the
                    // incarnation the entry event observed (CR 400.7).
                    || super::super::entry_observation::observed_object_reference_supported(
                        &effect["recipient"],
                    )
                    || player_reference_supported(&effect["recipient"])
                    || (value_kind(&effect["recipient"]) == Some("eachPermanent")
                        && (object_expression_supported(&effect["recipient"])
                            || super::super::object_relation_eval::related_collection_supported(
                                &effect["recipient"],
                            ))))
                && effect.get("source").is_none_or(|source| {
                    matches!(value_kind(source), Some("self" | "abilitySource"))
                        || target_reference_supported(source)
                })
                && effect.get("bindExcessAs").is_none_or(Value::is_string)
                && effect.get("bindRecipientAs").is_none_or(Value::is_string)
        }
        Some("dealDividedDamage") => {
            effect["targetsDecisionId"].is_string()
                && effect["divisionDecisionId"].is_string()
                && effect.get("source").is_none_or(target_reference_supported)
        }

        Some("chosenPermanentsDealPowerDamage") => {
            object_expression_supported(&effect["sources"])
                && target_reference_supported(&effect["recipient"])
        }
        _ => return None,
    };
    Some(supported)
}
