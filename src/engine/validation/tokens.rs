use serde_json::Value;

use super::super::{
    keyword_kind_supported, numeric_expression_supported, object_expression_supported,
    rule_supported, singular_player_reference_supported, target_reference_supported, value_kind,
};

/// The copied object of a `copyOf` token: the source, a declared target or a
/// bound singular object (CR 707.2).
fn copied_object_supported(object: &Value) -> bool {
    matches!(value_kind(object), Some("self" | "abilitySource"))
        || target_reference_supported(object)
        || object_expression_supported(object)
        || super::super::entry_observation::observed_object_reference_supported(object)
}

/// One copiable-value exception (CR 707.9b): the characteristic-change
/// operations without object or duration, and the removal of a supertype.
pub(in super::super) fn copy_exception_supported(exception: &Value) -> bool {
    let colors = |value: &Value| {
        value.as_array().is_some_and(|colors| {
            colors
                .iter()
                .all(|color| matches!(color.as_str(), Some("W" | "U" | "B" | "R" | "G")))
        })
    };
    match value_kind(exception) {
        Some("setBasePowerToughness") => {
            numeric_expression_supported(&exception["power"])
                && numeric_expression_supported(&exception["toughness"])
        }
        Some("removeSupertype") => {
            matches!(exception["supertype"].as_str(), Some("Legendary" | "Snow"))
        }
        Some("addCardType") => exception["cardType"].is_string(),
        Some("removeManaCost") => true,
        // CR 707.9b: a stated name or an added supertype of the copy.
        Some("setName") => exception["name"]
            .as_str()
            .is_some_and(|name| !name.is_empty()),
        Some("addSupertype") => {
            matches!(
                exception["supertype"].as_str(),
                Some("Legendary" | "Snow" | "Basic")
            )
        }
        Some("addSubtypes") => exception["subtypes"]
            .as_array()
            .is_some_and(|subtypes| !subtypes.is_empty() && subtypes.iter().all(Value::is_string)),
        Some("setColors") => colors(&exception["colors"]),
        Some("grantKeyword") => {
            keyword_kind_supported(&serde_json::json!({ "kind": exception["keyword"] }))
        }
        Some("grantAbility") => rule_supported(&exception["ability"]),
        _ => false,
    }
}

/// Validates `createTokens` whose token is a copy (CR 111.8, 707.2). Every
/// token copy is this one primitive; the former recipe kinds were migrated
/// to it. Any other effect is not owned here.
pub(in super::super) fn token_copy_effect_supported(effect: &Value) -> Option<bool> {
    if value_kind(effect) != Some("createTokens") || value_kind(&effect["token"]) != Some("copyOf")
    {
        return None;
    }
    let token = &effect["token"];
    Some(
        numeric_expression_supported(&effect["quantity"])
            // The copies have one creator and controller (CR 111.2); a
            // plural player reference is not a token creator.
            && effect
                .get("controller")
                .is_none_or(singular_player_reference_supported)
            && copied_object_supported(&token["object"])
            && token["except"]
                .as_array()
                .is_some_and(|exceptions| exceptions.iter().all(copy_exception_supported))
            && effect["counters"].as_array().is_none_or(|counters| {
                counters.iter().all(|counter| {
                    counter["counter"].is_string()
                        && numeric_expression_supported(&counter["count"])
                })
            })
            && ["tapped", "attacking", "sacrificeAtNextEndStep", "exileAtNextEndStep"]
                .iter()
                .all(|flag| effect.get(*flag).is_none_or(Value::is_boolean))
            && effect.get("bind").is_none_or(Value::is_string),
    )
}
