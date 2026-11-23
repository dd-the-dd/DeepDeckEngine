//! Lowering of the targeting frame of `become` (CR 115.1) to the generic
//! `becameTarget` event: the targeted subject (a selector union or the
//! source) and the targeting stack-object selector.
use super::super::*;

fn lower_controller(scope: ObjectControllerScope) -> Value {
    lower_object_controller_scope(scope)
}

fn lower_targeted_object_selector(selector: &TargetedObjectSelectorNode) -> Value {
    match selector.domain {
        TargetedObjectDomain::Permanent => json!({
            "kind": "permanents",
            "controller": lower_controller(selector.controller),
            "where": lower_permanent_object_predicate(&selector.predicate),
        }),
        TargetedObjectDomain::Spell => {
            let mut candidates = json!({
                "kind": "spells",
                "controller": lower_controller(selector.controller),
            });
            if let Some(predicate) = lower_spell_object_predicate(&selector.predicate) {
                candidates["where"] = predicate;
            }
            candidates
        }
    }
}

fn lower_targeting_stack_kind(kind: TargetingStackKind) -> Value {
    match kind {
        TargetingStackKind::Spell => json!({ "kind": "isSpell" }),
        TargetingStackKind::ActivatedAbility => json!({ "kind": "isActivatedAbility" }),
        TargetingStackKind::TriggeredAbility => json!({ "kind": "isTriggeredAbility" }),
    }
}

/// The spell or ability that targets: `stackItems` with its kinds and, when
/// written, its controller relation (absent: any controller).
pub(in crate::oracle::canonical) fn lower_targeting_stack_selector(
    selector: &TargetingStackSelectorNode,
) -> Value {
    let mut source_kinds = selector
        .kinds
        .iter()
        .copied()
        .map(lower_targeting_stack_kind)
        .collect::<Vec<_>>();
    let source_where = if source_kinds.len() == 1 {
        source_kinds.pop().expect("one stack-source kind")
    } else {
        or(source_kinds)
    };
    let mut lowered = json!({"kind": "stackItems"});
    if let Some(controller) = selector.controller {
        lowered["controller"] = lower_controller(controller);
    }
    lowered["where"] = source_where;
    lowered
}

pub(in crate::oracle::canonical) fn lower_became_target_event(
    event: &BecameTargetEventNode,
) -> Value {
    let mut object_sets = event
        .objects
        .iter()
        .map(lower_targeted_object_selector)
        .collect::<Vec<_>>();
    let object = if object_sets.len() == 1 {
        object_sets.pop().expect("one object selector")
    } else {
        json!({ "kind": "union", "sets": object_sets })
    };
    json!({
        "kind": "becameTarget",
        "object": object,
        "targetingStackObject": lower_targeting_stack_selector(&event.targeting_stack),
    })
}

/// The source itself as the targeted object (`this creature becomes the
/// target of ...`), matched by the generic targeting observer.
pub(in crate::oracle::canonical) fn lower_source_became_target_event(
    selector: &TargetingStackSelectorNode,
) -> Value {
    json!({
        "kind": "becameTarget",
        "object": self_ref(),
        "targetingStackObject": lower_targeting_stack_selector(selector),
    })
}
