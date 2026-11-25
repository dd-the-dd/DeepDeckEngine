//! Lowering of a continuous type change (`OBJECTS is/are TYPES`, CR 205.1,
//! 305.7, layer 4 per CR 613.1d) into the engine's independent battlefield
//! characteristic modifiers. Each added characteristic is its own primitive;
//! the affected-object selector is supplied unchanged by the static parent.
use super::super::*;

/// Returns one modifier per characteristic change, or the unsupported part.
/// `objects_are_lands` is the parent's proof that every affected object is a
/// land, which a replacing land-subtype change requires (CR 305.7).
///
/// - CR 205.1b: `in addition to its other types`, or a change that names an
///   `artifact creature`, retains every prior card type and subtype; each
///   named type is added (`addCardType`, `addSubtype`).
/// - CR 205.1a: otherwise the named card types replace the object's card
///   types (`setCardTypes`); the engine then drops every registered subtype
///   that no longer corresponds to a card type (CR 205.3d).
/// - CR 205.1b, 305.7: named subtypes without retention replace the
///   subtypes of their own class: creature types (`modifySubtypeSet`
///   `replace`), or one basic land type of a land (`setLandSubtype`, which
///   also replaces the land's rules text).
pub(in crate::oracle::canonical) fn lower_continuous_type_change(
    objects: &Value,
    objects_are_lands: bool,
    node: &TypeChangeCharacteristicsNode,
) -> Result<Vec<Value>, &'static str> {
    let additive = node.retention == TypeChangeRetention::InAdditionToOtherTypes
        || names_artifact_creature(&node.card_types);
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
    if node
        .card_types
        .iter()
        .any(|card_type| matches!(card_type, ObjectCardType::Kindred))
    {
        return Err("a card type the type-change primitives support");
    }
    if additive {
        for card_type in &node.card_types {
            modifiers.push(json!({
                "kind": "addCardType",
                "objects": objects.clone(),
                "cardType": card_type.ir_name(),
            }));
        }
        for subtype in &node.subtypes {
            modifiers.push(json!({
                "kind": "addSubtype",
                "objects": objects.clone(),
                "subtype": subtype.ir_name(),
            }));
        }
    } else {
        if !node.card_types.is_empty() {
            modifiers.push(json!({
                "kind": "setCardTypes",
                "objects": objects.clone(),
                "cardTypes": node
                    .card_types
                    .iter()
                    .map(|card_type| card_type.ir_name())
                    .collect::<Vec<_>>(),
            }));
        }
        // After a card-type change the objects are lands exactly when the
        // new card types include Land.
        let lands = if node.card_types.is_empty() {
            objects_are_lands
        } else {
            node.card_types.contains(&ObjectCardType::Land)
        };
        modifiers.extend(lower_replacing_subtypes(objects, lands, &node.subtypes)?);
    }
    if modifiers.is_empty() {
        return Err("a type-change characteristic with a battlefield primitive");
    }
    Ok(modifiers)
}

/// CR 205.1b: `[creature types] artifact creature` retains every prior card
/// type and subtype.
fn names_artifact_creature(card_types: &[ObjectCardType]) -> bool {
    card_types.contains(&ObjectCardType::Artifact) && card_types.contains(&ObjectCardType::Creature)
}

/// Named subtypes without retention, one replacement per subtype class
/// (CR 205.1b): creature types replace the creature types; one basic land
/// type replaces a land's land types (CR 305.7).
fn lower_replacing_subtypes(
    objects: &Value,
    objects_are_lands: bool,
    subtypes: &[TypeChangeSubtype],
) -> Result<Vec<Value>, &'static str> {
    let creature = subtypes
        .iter()
        .filter(|subtype| matches!(subtype, TypeChangeSubtype::Creature(_)))
        .map(|subtype| subtype.ir_name())
        .collect::<Vec<_>>();
    let land = subtypes
        .iter()
        .filter_map(|subtype| match subtype {
            TypeChangeSubtype::BasicLand(subtype) => Some(*subtype),
            TypeChangeSubtype::Creature(_) => None,
        })
        .collect::<Vec<_>>();
    let mut modifiers = Vec::new();
    if !creature.is_empty() {
        modifiers.push(json!({
            "kind": "modifySubtypeSet",
            "objects": objects.clone(),
            "subtypeClass": "creature",
            "mode": "replace",
            "subtypes": creature,
        }));
    }
    match land.as_slice() {
        [] => {}
        [subtype] if objects_are_lands => modifiers.push(json!({
            "kind": "setLandSubtype",
            "objects": objects.clone(),
            "subtype": subtype.ir_name(),
        })),
        _ => {
            return Err(
                "one basic land subtype replacing the subtypes of lands, or `in addition to`",
            );
        }
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
    let mut modifiers = if types.supertypes.is_empty()
        && types.subtypes.is_empty()
        && types.card_types.is_empty()
    {
        // `is a 3/3 with flying` states no type; the other characteristics
        // are read below.
        Vec::new()
    } else {
        lower_continuous_type_change(objects, objects_are_lands, &types)?
    };
    if node.characteristics.all_creature_types {
        // CR 205.3m: every creature type, in the creature subtype class.
        modifiers.push(json!({
            "kind": "modifySubtypeSet",
            "objects": objects.clone(),
            "subtypeClass": "creature",
            "mode": "all",
        }));
    }
    if node.characteristics.loses_all_abilities {
        // CR 613.1f: layer 6, on the same objects.
        modifiers.push(json!({ "kind": "loseAllAbilities", "objects": objects.clone() }));
    }
    if !node.characteristics.lost_keywords.is_empty() {
        modifiers.push(json!({
            "kind": "loseKeywords",
            "objects": objects.clone(),
            "keywords": node
                .characteristics
                .lost_keywords
                .iter()
                .map(|keyword| keyword.canonical_name())
                .collect::<Vec<_>>(),
        }));
    }
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
