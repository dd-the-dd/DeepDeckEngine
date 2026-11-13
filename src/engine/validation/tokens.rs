use serde_json::{Value, json};

use super::super::{
    integer_value, keyword_kind_supported, numeric_expression_supported,
    player_reference_supported, target_reference_supported, value_kind,
};

/// Validates primitive token-copy IR nodes.
///
/// Composite operations that perform another action before or after creating a
/// token intentionally remain outside this family until they are decomposed
/// into independent primitive effects.
pub(in super::super) fn token_copy_effect_supported(effect: &Value) -> Option<bool> {
    let supported = match value_kind(effect) {
        Some("createTokenCopy") => {
            player_reference_supported(&effect["controller"])
                && matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                )
                && integer_value(&effect["basePower"]).is_some()
                && integer_value(&effect["baseToughness"]).is_some()
        }
        Some("createTokenCopyOfPermanent") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && effect["grantKeywords"].as_array().is_none_or(|keywords| {
                    keywords.iter().all(|keyword| {
                        keyword
                            .as_str()
                            .is_some_and(|value| keyword_kind_supported(&json!({ "kind": value })))
                    })
                })
                && effect["exileAtNextEndStep"].is_boolean()
                && effect
                    .get("quantity")
                    .is_none_or(numeric_expression_supported)
                && effect.get("removeLegendary").is_none_or(Value::is_boolean)
                && effect
                    .get("grantKeywordsIfPermanentSpell")
                    .is_none_or(Value::is_boolean)
        }
        Some("createModifiedTokenCopy") => {
            target_reference_supported(&effect["object"])
                && effect
                    .get("basePower")
                    .is_none_or(|value| integer_value(value).is_some())
                && effect
                    .get("baseToughness")
                    .is_none_or(|value| integer_value(value).is_some())
                && ["addColors", "addTypes", "addSubtypes"].iter().all(|key| {
                    effect.get(*key).is_none_or(|value| {
                        value
                            .as_array()
                            .is_some_and(|values| values.iter().all(Value::is_string))
                    })
                })
                && effect["grantKeywords"].as_array().is_none_or(|keywords| {
                    keywords.iter().all(|keyword| {
                        keyword
                            .as_str()
                            .is_some_and(|value| keyword_kind_supported(&json!({ "kind": value })))
                    })
                })
                && effect
                    .get("sacrificeAtNextEndStep")
                    .is_none_or(Value::is_boolean)
                && effect.get("diesDrawCard").is_none_or(Value::is_boolean)
                && effect.get("removeLegendary").is_none_or(Value::is_boolean)
                && effect["counters"].as_array().is_none_or(|counters| {
                    counters.iter().all(|counter| {
                        counter["counter"].is_string()
                            && numeric_expression_supported(&counter["count"])
                    })
                })
        }
        _ => return None,
    };
    Some(supported)
}
