//! Lowering of a continuous type change (`OBJECTS is/are TYPES`, CR 205.1,
//! 305.7, layer 4 per CR 613.1d) into the engine's independent battlefield
//! characteristic modifiers. Each added characteristic is its own primitive;
//! the affected-object selector is supplied unchanged by the static parent.
use super::super::*;

/// Returns one modifier per characteristic change, or the unsupported part.
/// `objects_are_lands` is the parent's proof that every affected object is a
/// land, which a replacing land-subtype change requires (CR 305.7).
pub(in crate::oracle::canonical) fn lower_continuous_type_change(
    objects: &Value,
    objects_are_lands: bool,
    node: &TypeChangeCharacteristicsNode,
) -> Result<Vec<Value>, &'static str> {
    let additive = node.retention == TypeChangeRetention::InAdditionToOtherTypes;
    let mut modifiers = Vec::new();
    for supertype in &node.supertypes {
        // CR 205.4: a supertype is added; no supertype-setting form exists.
        modifiers.push(json!({
            "kind": "addSupertype",
            "objects": objects.clone(),
            "supertype": supertype.ir_name(),
        }));
    }
    if node.all_basic_land_types {
        if !additive {
            return Err("`in addition to their other types` after `every basic land type`");
        }
        modifiers.push(json!({
            "kind": "addAllBasicLandTypes",
            "objects": objects.clone(),
        }));
    }
    if !node.card_types.is_empty() && !additive {
        // CR 205.1a: setting card types replaces the old ones; only the
        // additive form has a battlefield primitive.
        return Err("`in addition to its other types` after a card-type change");
    }
    for card_type in &node.card_types {
        if matches!(card_type, ObjectCardType::Kindred) {
            return Err("a card type the additive type-change primitive supports");
        }
        modifiers.push(json!({
            "kind": "addCardType",
            "objects": objects.clone(),
            "cardType": card_type.ir_name(),
        }));
    }
    if additive {
        for subtype in &node.subtypes {
            modifiers.push(json!({
                "kind": "addSubtype",
                "objects": objects.clone(),
                "subtype": subtype.ir_name(),
            }));
        }
    } else {
        match node.subtypes.as_slice() {
            [] => {}
            // CR 305.7: a land whose subtype is set to a basic land type loses
            // its other land types and rules-text abilities and has the
            // intrinsic mana ability of the new type.
            [TypeChangeSubtype::BasicLand(subtype)]
                if node.card_types.is_empty() && objects_are_lands =>
            {
                modifiers.push(json!({
                    "kind": "setLandSubtype",
                    "objects": objects.clone(),
                    "subtype": subtype.ir_name(),
                }));
            }
            _ => {
                return Err(
                    "one basic land subtype replacing the subtypes of lands, or `in addition to`",
                );
            }
        }
    }
    if modifiers.is_empty() {
        return Err("a type-change characteristic with a battlefield primitive");
    }
    Ok(modifiers)
}

#[cfg(test)]
#[path = "continuous_type_change_tests.rs"]
mod tests;
