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
    let mut source_kinds = event
        .targeting_stack
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
    json!({
        "kind": "becameTarget",
        "object": object,
        "targetingStackObject": {
            "kind": "stackItems",
            "controller": lower_controller(event.targeting_stack.controller),
            "where": source_where,
        },
    })
}

pub(in crate::oracle::canonical) fn lower_targetless_trigger_effects(
    effects: &EffectSequenceNode,
) -> Result<Vec<Value>, TargetlessLoweringError> {
    effects
        .effects
        .iter()
        .map(|effect| lower_targetless_local_effect(effect, TargetlessDeclarationSupport::None))
        .collect()
}
