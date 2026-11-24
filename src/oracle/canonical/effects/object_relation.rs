//! Lowering of the object relations of the shared query (`attached to that
//! creature`, `blocking it`, `that shares a creature type with it`).
//!
//! A relation atom names its anchor with the shared object-expression
//! vocabulary (`self`, `triggeringPermanent`, `chosenTarget`, an
//! `eachPermanent` query). The predicate lowering is pure and has no target
//! declaration, so a declared-target anchor is written as a symbolic
//! `declaredTargetAnchor` slot; the program lowering that owns the
//! declaration resolves it (`resolve_relation_target_anchors`). A lowering
//! path without that step keeps the symbolic slot, which no engine consumer
//! accepts: the rule is refused rather than read against another object.
use super::super::*;

/// The symbolic kind of a declared-target anchor before its declaration is
/// known.
pub(in crate::oracle::canonical) const DECLARED_TARGET_ANCHOR: &str = "declaredTargetAnchor";

/// The IR atom of one relation: its kind, its anchor and, for a share, the
/// characteristic compared.
pub(in crate::oracle::canonical) fn lower_related_object(related: &RelatedObject) -> Value {
    let mut atom = json!({
        "kind": related.kind.ir_kind(),
        "object": lower_relation_anchor(&related.anchor),
    });
    if let ObjectRelationKind::Shares(characteristic) = related.kind {
        atom["characteristic"] = json!(characteristic.ir_name());
    }
    atom
}

fn lower_relation_anchor(anchor: &RelationAnchor) -> Value {
    match anchor {
        RelationAnchor::Object(PermanentEffectReference::Target(binding)) => json!({
            "kind": DECLARED_TARGET_ANCHOR,
            "effectIndex": binding.effect_index,
            "slotIndex": binding.slot_index,
        }),
        RelationAnchor::Object(PermanentEffectReference::TargetSet(binding)) => json!({
            "kind": DECLARED_TARGET_ANCHOR,
            "effectIndex": binding.effect_index,
            "slotIndex": binding.slot_index,
            "set": true,
        }),
        // The relative slot admits only references with a value here.
        RelationAnchor::Object(reference) => bound_permanent_reference_value(*reference, &[])
            .unwrap_or_else(|| json!({ "kind": DECLARED_TARGET_ANCHOR })),
        RelationAnchor::Query(query) => {
            let mut objects = json!({
                "kind": "eachPermanent",
                "where": lower_permanent_object_predicate(&query.predicate),
            });
            if let Some(controller) = query.controller {
                objects["player"] = lower_object_controller_scope(controller);
            }
            objects
        }
    }
}

/// The declared-target reference a symbolic anchor names, if `value` is one.
fn declared_target_anchor(value: &Value) -> Option<Option<PermanentEffectReference>> {
    if value["kind"].as_str() != Some(DECLARED_TARGET_ANCHOR) {
        return None;
    }
    let index = |field: &str| {
        value[field]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
    };
    let binding = index("effectIndex")
        .zip(index("slotIndex"))
        .map(|(effect_index, slot_index)| TargetBinding {
            effect_index,
            slot_index,
        });
    Some(binding.map(|binding| {
        if value["set"].as_bool() == Some(true) {
            PermanentEffectReference::TargetSet(binding)
        } else {
            PermanentEffectReference::Target(binding)
        }
    }))
}

fn resolve_anchors_in(
    cursor: &mut OracleTokenCursor<'_>,
    value: &mut Value,
    bindings: &[(TargetBinding, String)],
) -> MachineResult<()> {
    if let Some(reference) = declared_target_anchor(value) {
        cursor.enter("lower.object-relation.anchor-target");
        let resolved = reference
            .and_then(|reference| bound_permanent_reference_value(reference, bindings))
            .ok_or_else(|| cursor.fail("the declared target slot a relation anchor names"))?;
        *value = resolved;
        return Ok(());
    }
    match value {
        Value::Object(fields) => {
            for child in fields.values_mut() {
                resolve_anchors_in(cursor, child, bindings)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                resolve_anchors_in(cursor, item, bindings)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Resolves every declared-target anchor of the lowered program against the
/// program's own declarations (CR 608.2b: the relation reads the object the
/// target slot designated). A slot the program did not declare fails here.
pub(in crate::oracle::canonical) fn resolve_relation_target_anchors(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut [Value],
    bindings: &[(TargetBinding, String)],
) -> MachineResult<()> {
    for effect in effects.iter_mut() {
        resolve_anchors_in(cursor, effect, bindings)?;
    }
    Ok(())
}
