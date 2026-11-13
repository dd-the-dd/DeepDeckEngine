use serde_json::Value;

use super::super::{
    card_filter_supported, integer_value, numeric_expression_supported,
    object_expression_supported, player_reference_supported, target_reference_supported,
    value_kind,
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
                    || player_reference_supported(&effect["recipient"])
                    || (value_kind(&effect["recipient"]) == Some("eachPermanent")
                        && card_filter_supported(&effect["recipient"]["where"])
                        && effect["recipient"]
                            .get("player")
                            .is_none_or(player_reference_supported)))
                && effect.get("bindExcessAs").is_none_or(Value::is_string)
                && effect.get("bindRecipientAs").is_none_or(Value::is_string)
        }
        Some("dealDividedDamage") => {
            effect["targetsDecisionId"].is_string()
                && effect["divisionDecisionId"].is_string()
                && effect.get("source").is_none_or(target_reference_supported)
        }
        Some("dealDamageToEachOpponent") => {
            numeric_expression_supported(&effect["amount"])
                && effect.get("source").is_none_or(target_reference_supported)
        }
        Some("dealDamageToEachOpponentAndCreaturesTheyControl")
        | Some("dealDamageToEachOpponentAndPlaneswalker") => {
            numeric_expression_supported(&effect["amount"])
        }
        Some("dealDamageToEachCreatureExceptControlledTokens") => {
            player_reference_supported(&effect["player"])
                && numeric_expression_supported(&effect["amount"])
        }
        Some("dealDamageByPermanentCountEachPlayer") => {
            card_filter_supported(&effect["where"])
                && integer_value(&effect["factor"]).is_some_and(|factor| factor > 0)
        }
        Some("chosenPermanentsDealPowerDamage") => {
            object_expression_supported(&effect["sources"])
                && target_reference_supported(&effect["recipient"])
        }
        _ => return None,
    };
    Some(supported)
}
