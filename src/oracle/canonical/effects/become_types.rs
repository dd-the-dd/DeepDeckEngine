//! Lowering of the shared characteristic-change frame into independent
//! primitive characteristic changes, each applied to the same object in
//! Oracle order and in its own layer (CR 613.1a-f, 613.4b):
//!
//! - layer 1: `becomeCopyOfPermanent` (CR 707.2);
//! - layer 4: `addCardType` (types retained, CR 205.1b) or `setCardTypes`
//!   (CR 205.1a), then `modifySubtypeSet` (`add`, `replace` or `all`) per
//!   subtype class, a chosen subtype reading its resolution decision;
//! - layer 5: `setColors` (CR 105.3), a chosen color set reading its decision;
//! - layer 6: `grantKeyword` / `grantAbility` / `removeKeywordsFromPermanents`
//!   / ability loss;
//! - layer 7b: `setBasePowerToughness` with shared numeric expressions.
use super::super::*;

fn lower_become_types_duration(duration: Option<&EffectDurationNode>) -> Option<Value> {
    match duration {
        None => Some(json!({ "kind": "permanent" })),
        Some(EffectDurationNode::UntilEndOfCurrentTurn) => {
            Some(json!({ "kind": "untilEndOfCurrentTurn" }))
        }
        Some(
            EffectDurationNode::UntilNextTurn { .. }
            | EffectDurationNode::UntilEndOfNextTurn { .. }
            | EffectDurationNode::Indefinite
            | EffectDurationNode::WhileSourceControlled,
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

/// The five color symbols of `all colors` (CR 105.1).
const ALL_COLOR_SYMBOLS: [&str; 5] = ["W", "U", "B", "R", "G"];

/// One base power/toughness component: a literal, `X` (a decision the
/// sequence's `where X` or the face's mana X binds) or another expression of
/// the shared numeric lowering.
fn lower_stat_component(expression: &NumericExpression) -> Result<Value, &'static str> {
    if let NumericExpression::Atom(NumericAtom::Integer(value)) = expression
        && i32::try_from(*value).is_err()
    {
        return Err("a base power/toughness representable by runtime i32 quantities");
    }
    lower_numeric_expression(expression)
        .ok_or("a base power/toughness expression the shared numeric lowering represents")
}

/// The copiable-value modifications of a characteristic change (CR 707.9b),
/// with no object and no duration: the same operation kinds as a resolved
/// change, so a copy's `except` list and a `becomes` share one vocabulary.
pub(in crate::oracle::canonical) fn lower_copiable_characteristic_changes<Subject>(
    node: &BecomeTypesNode<Subject>,
) -> Result<Vec<Value>, &'static str> {
    let mut operations = Vec::new();
    if node.chosen_subtype.is_some()
        || node.copy_of.is_some()
        || node.characteristics.loses_all_abilities
        || !node.characteristics.lost_keywords.is_empty()
        || node.characteristics.all_creature_types
    {
        return Err("copiable-value exceptions the token copy supports");
    }
    for card_type in &node.card_types {
        operations.push(json!({ "kind": "addCardType", "cardType": card_type.ir_name() }));
    }
    if !node.subtypes.is_empty() {
        operations.push(json!({
            "kind": "addSubtypes",
            "subtypes": node.subtypes.iter().map(|subtype| subtype.ir_name()).collect::<Vec<_>>(),
        }));
    }
    match &node.colors {
        None => {}
        Some(ColorComplementNode::Colors(colors)) => operations.push(json!({
            "kind": "setColors",
            "colors": colors.colors.iter().map(|color| color.ir_symbol()).collect::<Vec<_>>(),
        })),
        Some(ColorComplementNode::AllColors) => operations.push(json!({
            "kind": "setColors",
            "colors": ALL_COLOR_SYMBOLS,
        })),
        Some(ColorComplementNode::Chosen(_)) => {
            return Err("a stated color set in a copy exception");
        }
    }
    for keyword in &node.characteristics.keywords {
        operations.push(json!({ "kind": "grantKeyword", "keyword": keyword.canonical_name() }));
    }
    for ability in &node.characteristics.abilities {
        operations.push(json!({ "kind": "grantAbility", "ability": ability.rule }));
    }
    if let Some(statistics) = node.base_power_toughness() {
        operations.push(json!({
            "kind": "setBasePowerToughness",
            "power": lower_stat_component(&statistics.power)?,
            "toughness": lower_stat_component(&statistics.toughness)?,
        }));
    }
    Ok(operations)
}

/// `binding_id` names the locked-in set of a collection subject: every
/// primitive of one effect affects the objects selected when it begins
/// (CR 611.2c), even if an earlier primitive changes what the query matches.
/// `copy` is the lowered object of `becomes a copy of OBJECT`.
pub(in crate::oracle::canonical) fn lower_become_types_to_object(
    node: &BecomeTypesNode<PermanentOperandNode>,
    object: &Value,
    copy: Option<&Value>,
    binding_id: &str,
) -> Result<Vec<Value>, &'static str> {
    lower_become_types_to_object_in_scope(node, object, copy, binding_id, None)
}

/// The same lowering when the change's own declared subject is the target
/// slot `subject_binding` of the program: a possessive `its` in a stated
/// amount (`with power and toughness each equal to its mana value`) reads
/// that object, determined once as the effect begins (CR 608.2h).
pub(in crate::oracle::canonical) fn lower_become_types_to_object_in_scope(
    node: &BecomeTypesNode<PermanentOperandNode>,
    object: &Value,
    copy: Option<&Value>,
    binding_id: &str,
    subject_binding: Option<TargetBinding>,
) -> Result<Vec<Value>, &'static str> {
    let subject_slot = match (&node.objects, subject_binding, object["id"].as_str()) {
        (PermanentOperandNode::Targets(targets), Some(binding), Some(id))
            if object["kind"] == "chosenTarget" && targets.slots.len() == 1 =>
        {
            Some(NumericTargetSlot {
                binding,
                target_id: id.to_string(),
                selector: targets.slots[0].selector.clone(),
                requires_post_zone_change_lki: false,
            })
        }
        _ => None,
    };
    let stat_component = |expression: &NumericExpression| match &subject_slot {
        // The shared target lowering also refuses a literal beyond i32.
        Some(slot) => lower_numeric_expression_for_targets(expression, std::slice::from_ref(slot))
            .map_err(|_| "a base power/toughness amount that reads the changed object"),
        None => lower_stat_component(expression),
    };
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
    if node.copy_of.is_some() {
        // CR 707.2: layer 1 copies the copiable values; the copy effect has
        // no other characteristic of its own.
        let copy = copy.ok_or("the lowered object of `becomes a copy of`")?;
        let mut effect = json!({
            "kind": "becomeCopyOfPermanent",
            "object": object,
            "copy": copy,
            "retainResolvingAbility": false,
        });
        if duration["kind"] == "untilEndOfCurrentTurn" {
            effect["duration"] = duration.clone();
        }
        effects.push(effect);
    }
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
    if !node.characteristics.lost_keywords.is_empty() {
        effects.push(json!({
            "kind": "removeKeywordsFromPermanents",
            "objects": object,
            "keywords": node
                .characteristics
                .lost_keywords
                .iter()
                .map(|keyword| keyword.canonical_name())
                .collect::<Vec<_>>(),
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
    let mode = if retains_prior { "add" } else { "replace" };
    for (class, subtypes) in [
        (
            "creature",
            node.subtypes
                .iter()
                .filter(|subtype| matches!(subtype, TypeChangeSubtype::Creature(_)))
                .map(|subtype| subtype.ir_name())
                .collect::<Vec<_>>(),
        ),
        (
            "land",
            node.subtypes
                .iter()
                .filter(|subtype| matches!(subtype, TypeChangeSubtype::BasicLand(_)))
                .map(|subtype| subtype.ir_name())
                .collect::<Vec<_>>(),
        ),
    ] {
        if !subtypes.is_empty() {
            effects.push(json!({
                "kind": "modifySubtypeSet",
                "object": object,
                "subtypeClass": class,
                "mode": mode,
                "subtypes": subtypes,
                "duration": duration,
            }));
        }
    }
    if let Some(chosen) = node.chosen_subtype {
        // The controller chooses as the effect resolves (CR 608.2d); the
        // subtype change reads that decision.
        let (choice, class) = match chosen {
            ChosenSubtypeClass::CreatureType => ("chooseCreatureType", "creature"),
            ChosenSubtypeClass::BasicLandType => ("chooseBasicLandType", "land"),
        };
        let id = format!("{binding_id}:chosenSubtype");
        effects.push(json!({ "kind": choice, "id": id, "player": controller() }));
        effects.push(json!({
            "kind": "modifySubtypeSet",
            "object": object,
            "subtypeClass": class,
            "mode": mode,
            "subtypes": decision_result(&id),
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
    match &node.colors {
        None => {}
        Some(ColorComplementNode::Colors(colors)) => effects.push(json!({
            "kind": "setColors",
            "object": object,
            "colors": colors.colors.iter().map(|color| color.ir_symbol()).collect::<Vec<_>>(),
            "duration": duration,
        })),
        Some(ColorComplementNode::AllColors) => effects.push(json!({
            "kind": "setColors",
            "object": object,
            "colors": ALL_COLOR_SYMBOLS,
            "duration": duration,
        })),
        Some(ColorComplementNode::Chosen(count)) => {
            let id = format!("{binding_id}:chosenColors");
            effects.push(json!({
                "kind": "chooseColors",
                "id": id,
                "player": controller(),
                "minimum": integer(1),
                "maximum": integer(match count {
                    ChosenColorCount::One => 1,
                    ChosenColorCount::OneOrMore => 5,
                }),
            }));
            effects.push(json!({
                "kind": "setColors",
                "object": object,
                "colors": decision_result(&id),
                "duration": duration,
            }));
        }
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
            "power": stat_component(&statistics.power)?,
            "toughness": stat_component(&statistics.toughness)?,
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
