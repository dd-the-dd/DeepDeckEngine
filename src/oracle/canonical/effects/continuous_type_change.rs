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

/// The characteristic-change complement of a static copula (`is a 3/3
/// black Ooze creature. It's still a land.`): its types through the same
/// type-change lowering, then one battlefield modifier per other stated
/// characteristic (layer 5 colors, layer 6 keywords, layer 7b base power and
/// toughness). A retained prior type (CR 205.1b) retains every prior type,
/// so the new types are additive. Characteristics without a static
/// battlefield primitive are refused, never dropped.
pub(in crate::oracle::canonical) fn lower_continuous_characteristic_complement(
    objects: &Value,
    objects_are_lands: bool,
    node: &BecomeTypesNode<()>,
) -> Result<Vec<Value>, &'static str> {
    if node.copy_of.is_some()
        || node.copy_exceptions.is_some()
        || node.copy_name.is_some()
        || node.chosen_subtype.is_some()
        || node.duration.is_some()
        || node.characteristics.all_creature_types
        || node.characteristics.loses_all_abilities
        || !node.characteristics.lost_keywords.is_empty()
        || node.characteristics.base_power.is_some()
        || node.characteristics.switches_power_toughness
    {
        return Err("a characteristic a static copula lowers to a battlefield modifier");
    }
    let types = TypeChangeCharacteristicsNode {
        supertypes: node.supertypes.clone(),
        subtypes: node.subtypes.clone(),
        card_types: node.card_types.clone(),
        all_basic_land_types: false,
        retention: if node.retention.retains_prior_types() {
            TypeChangeRetention::InAdditionToOtherTypes
        } else {
            TypeChangeRetention::ReplacesSameClass
        },
    };
    let mut modifiers = lower_continuous_type_change(objects, objects_are_lands, &types)?;
    match &node.colors {
        None => {}
        Some(ColorComplementNode::Colors(colors)) => modifiers.push(json!({
            "kind": "setColor",
            "objects": objects.clone(),
            "colors": colors.colors.iter().map(|color| color.ir_symbol()).collect::<Vec<_>>(),
        })),
        Some(ColorComplementNode::AllColors) => {
            modifiers.push(json!({ "kind": "setAllColors", "objects": objects.clone() }));
        }
        Some(ColorComplementNode::Chosen(_)) => {
            return Err("a stated color set in a static copula");
        }
    }
    if let Some(statistics) = node.base_power_toughness() {
        let literal = statistics
            .literal()
            .ok_or("a literal base power and toughness in a static copula")?;
        modifiers.push(json!({
            "kind": "setBasePowerToughness",
            "objects": objects.clone(),
            "power": integer(literal.power),
            "toughness": integer(literal.toughness),
        }));
    }
    for keyword in &node.characteristics.keywords {
        modifiers.push(json!({
            "kind": "grantKeyword",
            "objects": objects.clone(),
            "keyword": keyword.canonical_name(),
        }));
    }
    Ok(modifiers)
}

#[cfg(test)]
#[path = "continuous_type_change_tests.rs"]
mod tests;
