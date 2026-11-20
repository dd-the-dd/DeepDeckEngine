//! Lowering of `OBJECT becomes a [SUBTYPES] [CARD TYPES] [in addition to its
//! other types]` into independent primitive characteristic changes (CR 205.1,
//! layer 4). Subtype and card-type changes stay separate primitives; the
//! retention clause selects `replace` versus `add` for the subtype class.
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
            | EffectDurationNode::WhileSourceControlled,
        ) => None,
    }
}

pub(in crate::oracle::canonical) fn lower_become_types_to_object(
    node: &BecomeTypesNode<PermanentOperandNode>,
    object: &Value,
) -> Result<Vec<Value>, &'static str> {
    let duration = lower_become_types_duration(node.duration.as_ref())
        .ok_or("a type-change duration that the runtime can end (`until end of turn` or none)")?;
    let in_addition = node.retention == TypeChangeRetention::InAdditionToOtherTypes;
    if !node.card_types.is_empty() && !in_addition {
        // CR 205.1a: setting card types replaces the old ones and prunes
        // subtypes that no longer apply; only the additive form is runnable.
        return Err("`in addition to its other types` after a card-type change");
    }
    let mut effects = Vec::new();
    for card_type in &node.card_types {
        if matches!(card_type, ObjectCardType::Kindred) {
            return Err("a card type the additive type-change primitive supports");
        }
        effects.push(json!({
            "kind": "addCardType",
            "object": object,
            "cardType": card_type.ir_name(),
            "retainExistingTypes": true,
            "duration": duration,
        }));
    }
    if !node.subtypes.is_empty() {
        effects.push(json!({
            "kind": "modifySubtypeSet",
            "object": object,
            "subtypeClass": "creature",
            "mode": if in_addition { "add" } else { "replace" },
            "subtypes": node.subtypes.iter().map(|subtype| subtype.ir_name()).collect::<Vec<_>>(),
            "duration": duration,
        }));
    }
    Ok(effects)
}

#[cfg(test)]
#[path = "become_types_tests.rs"]
mod tests;
