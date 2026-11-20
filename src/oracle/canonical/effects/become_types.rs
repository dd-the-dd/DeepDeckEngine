//! Lowering of the shared `OBJECT becomes a ...` frame into independent
//! primitive characteristic changes, each applied to the same object in
//! Oracle order and in its own layer (CR 613.1d-f, 613.4b):
//!
//! - layer 4: `addCardType` (types retained, CR 205.1b) or `setCardTypes`
//!   (CR 205.1a), then `modifySubtypeSet` (`add`, `replace` or `all`);
//! - layer 5: `setColors` (CR 105.3);
//! - layer 6: `grantKeyword` / `grantAbility`;
//! - layer 7b: `setBasePowerToughness`.
use super::super::*;

fn lower_become_types_duration(duration: Option<&EffectDurationNode>) -> Option<Value> {
    match duration {
        None => Some(json!({ "kind": "permanent" })),
        Some(EffectDurationNode::UntilEndOfCurrentTurn) => {
            Some(json!({ "kind": "untilEndOfCurrentTurn" }))
        }
        Some(
            EffectDurationNode::UntilNextTurn { .. }
            | EffectDurationNode::UntilEndOfNextTurn { .. },
        ) => None,
    }
}

/// CR 205.1b: `[creature types] artifact creature` retains every prior card
/// type and every prior non-creature subtype.
fn names_artifact_creature(card_types: &[ObjectCardType]) -> bool {
    card_types.contains(&ObjectCardType::Artifact) && card_types.contains(&ObjectCardType::Creature)
}

/// A quoted ability becomes one complete rule of the affected object for the
/// effect's duration (CR 113.1, 613.1f). The runtime installs a temporary rule
/// of any kind on a single object, and a permanent one for the rule kinds it
/// can execute from a permanent's own rules.
fn quoted_ability_grantable(rule: &Value, object: &Value, duration: &Value) -> bool {
    let single = matches!(
        object["kind"].as_str(),
        Some("chosenTarget" | "self" | "abilitySource")
    );
    single
        && matches!(
            duration["kind"].as_str(),
            Some("untilEndOfCurrentTurn" | "permanent")
        )
        && matches!(
            rule["kind"].as_str(),
            Some("triggeredAbility" | "activatedAbility" | "manaAbility" | "staticAbility")
        )
}

/// `binding_id` names the locked-in set of a collection subject: every
/// primitive of one effect affects the objects selected when it begins
/// (CR 611.2c), even if an earlier primitive changes what the query matches.
pub(in crate::oracle::canonical) fn lower_become_types_to_object(
    node: &BecomeTypesNode<PermanentOperandNode>,
    object: &Value,
    binding_id: &str,
) -> Result<Vec<Value>, &'static str> {
    let mut effects = Vec::new();
    let locked_set;
    let object = if matches!(
        object["kind"].as_str(),
        Some("chosenTarget" | "self" | "abilitySource")
    ) {
        object
    } else {
        effects.push(json!({ "kind": "bind", "id": binding_id, "value": object }));
        locked_set = json!({ "kind": "boundObjects", "binding": binding_id });
        &locked_set
    };
    let duration = lower_become_types_duration(node.duration.as_ref())
        .ok_or("a type-change duration that the runtime can end (`until end of turn` or none)")?;
    let retains_prior = node.retention.retains_prior_types();
    let artifact_creature = names_artifact_creature(&node.card_types);
    if node.characteristics.loses_all_abilities {
        effects.push(json!({
            "kind": if duration["kind"] == "permanent" {
                "loseAllAbilitiesPermanently"
            } else {
                "installLoseAllAbilities"
            },
            "object": object,
            "duration": duration,
        }));
    }
    if node
        .card_types
        .iter()
        .any(|card_type| matches!(card_type, ObjectCardType::Kindred))
    {
        return Err("a card type the type-change primitives support");
    }
    if retains_prior || artifact_creature {
        for card_type in &node.card_types {
            effects.push(json!({
                "kind": "addCardType",
                "object": object,
                "cardType": card_type.ir_name(),
                "retainExistingTypes": true,
                "duration": duration,
            }));
        }
    } else if !node.card_types.is_empty() {
        effects.push(json!({
            "kind": "setCardTypes",
            "object": object,
            "cardTypes": node
                .card_types
                .iter()
                .map(|card_type| card_type.ir_name())
                .collect::<Vec<_>>(),
            "duration": duration,
        }));
    }
    if !node.subtypes.is_empty() {
        effects.push(json!({
            "kind": "modifySubtypeSet",
            "object": object,
            "subtypeClass": "creature",
            "mode": if retains_prior { "add" } else { "replace" },
            "subtypes": node.subtypes.iter().map(|subtype| subtype.ir_name()).collect::<Vec<_>>(),
            "duration": duration,
        }));
    }
    if node.characteristics.all_creature_types {
        effects.push(json!({
            "kind": "modifySubtypeSet",
            "object": object,
            "subtypeClass": "creature",
            "mode": "all",
            "duration": duration,
        }));
    }
    if let Some(colors) = &node.colors {
        effects.push(json!({
            "kind": "setColors",
            "object": object,
            "colors": colors.colors.iter().map(|color| color.ir_symbol()).collect::<Vec<_>>(),
            "duration": duration,
        }));
    }
    for keyword in &node.characteristics.keywords {
        effects.push(json!({
            "kind": "grantKeyword",
            "object": object,
            "keyword": keyword.canonical_name(),
            "duration": duration,
        }));
    }
    for ability in &node.characteristics.abilities {
        if !quoted_ability_grantable(&ability.rule, object, &duration) {
            return Err("a quoted ability the runtime can grant to this object for this duration");
        }
        effects.push(json!({
            "kind": "grantAbility",
            "object": object,
            "ability": ability.rule,
            "duration": duration,
        }));
    }
    if let Some(statistics) = node.base_power_toughness() {
        effects.push(json!({
            "kind": "setBasePowerToughness",
            "object": object,
            "power": integer(statistics.power),
            "toughness": integer(statistics.toughness),
            "duration": duration,
        }));
    }
    Ok(effects)
}

/// The runtime removes every ability of an object that lost all abilities,
/// including ones granted later (CR 613.7 would keep a later grant). Until
/// layer-6 effects carry timestamps, a grant after the loss is refused.
pub(in crate::oracle::canonical) fn no_grant_after_ability_loss(effects: &[Value]) -> bool {
    let mut lost = Vec::new();
    for effect in effects {
        match effect["kind"].as_str() {
            Some("installLoseAllAbilities" | "loseAllAbilitiesPermanently") => {
                lost.push(&effect["object"]);
            }
            Some("grantKeyword" | "grantAbility") if lost.contains(&&effect["object"]) => {
                return false;
            }
            _ => {}
        }
    }
    true
}

#[cfg(test)]
#[path = "become_types_tests.rs"]
mod tests;
