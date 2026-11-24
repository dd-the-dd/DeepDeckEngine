//! One subject-lowering boundary for ordinary permanent actions.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_bound_permanent_reference(
    cursor: &OracleTokenCursor<'_>,
    reference: PermanentEffectReference,
    bindings: &[(TargetBinding, String)],
) -> MachineResult<Value> {
    bound_permanent_reference_value(reference, bindings).ok_or_else(|| {
        cursor.fail("the previously declared permanent target slot in this lexical scope")
    })
}

/// The IR of an object the binding environment named. A target reference
/// needs the slot the program declared; without it there is no value.
pub(in crate::oracle::canonical) fn bound_permanent_reference_value(
    reference: PermanentEffectReference,
    bindings: &[(TargetBinding, String)],
) -> Option<Value> {
    match reference {
        PermanentEffectReference::Source(LocalObjectReference::SourceSelf) => Some(self_ref()),
        PermanentEffectReference::Observed(TypedObservedEventObject::EnteringPermanent) => {
            Some(json!({"kind":"observedEventObject","event":"permanentEntered"}))
        }
        PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens) => {
            Some(json!({"kind":"boundObjects","binding":CREATED_TOKENS_BINDING}))
        }
        PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield) => {
            Some(json!({"kind":"singleBoundObject","binding":MOVED_ONTO_BATTLEFIELD_BINDING}))
        }
        PermanentEffectReference::Observed(TypedObservedEventObject::CombatCounterpart) => {
            Some(json!({"kind":"combatCounterpart"}))
        }
        PermanentEffectReference::Observed(TypedObservedEventObject::EventPermanent) => {
            Some(json!({"kind":"triggeringPermanent"}))
        }
        PermanentEffectReference::Attached(_) => {
            Some(json!({"kind":"attachedPermanent","attachment":self_ref()}))
        }
        PermanentEffectReference::Target(target) => bindings
            .iter()
            .find_map(|(binding, id)| (*binding == target).then(|| chosen_target(id))),
        // `those creatures`: every object the plural slot declared.
        PermanentEffectReference::TargetSet(target) => {
            let key = target_set_binding_key(target);
            bindings.iter().find_map(|(binding, id)| {
                (*binding == key).then(|| json!({"kind":"chosenTargets","id":id}))
            })
        }
    }
}

/// The key a plural slot's binding is recorded under. Plural slots live in a
/// key space disjoint from singular slot keys, so a singular reference
/// (`it`, `its power`) never resolves to a set of targets.
pub(super) fn target_set_binding_key(slot: TargetBinding) -> TargetBinding {
    const TARGET_SET_SLOT_OFFSET: usize = 1 << 16;
    TargetBinding {
        effect_index: slot.effect_index,
        slot_index: slot.slot_index + TARGET_SET_SLOT_OFFSET,
    }
}

/// The IR of a player the binding environment named: the event's player, a
/// player declared earlier in the program, or a player related to a bound
/// object (CR 108.3, 109.4). Never a new declaration.
pub(in crate::oracle::canonical) fn bound_player_reference_value(
    reference: BoundPlayerReference,
    object_bindings: &[(TargetBinding, String)],
    player_bindings: &[(PlayerTargetBinding, String)],
) -> Option<Value> {
    match reference {
        BoundPlayerReference::EventPlayer => Some(json!({"kind":"triggeringPlayer"})),
        BoundPlayerReference::EventSpellController => Some(json!({
            "kind": "controllerOf",
            "object": {"kind": "triggeringStackObject"},
        })),
        BoundPlayerReference::Bound(binding) => player_bindings
            .iter()
            .find_map(|(key, id)| (key.effect_index == binding.0).then(|| chosen_target(id))),
        BoundPlayerReference::ObjectRelation { relation, object } => {
            let object = bound_permanent_reference_value(object, object_bindings)?;
            Some(json!({
                "kind": match relation {
                    ObjectPlayerRelation::Controller => "controllerOf",
                    ObjectPlayerRelation::Owner => "ownerOf",
                },
                "object": object,
            }))
        }
        // The defending player of a combat check is bound only while a
        // static combat restriction is checked, never in an effect program.
        BoundPlayerReference::DefendingPlayer => None,
    }
}

pub(in crate::oracle::canonical) fn ordinary_action_targets(
    effect: &LocalEffectNode,
) -> Option<&TargetListNode> {
    let operand = match effect {
        LocalEffectNode::ModifyStats(node) => &node.targets,
        LocalEffectNode::TemporaryKeywordGrant(node) => &node.objects,
        LocalEffectNode::BecomeTypes(node) => &node.objects,
        LocalEffectNode::PutCounters(node) => &node.permanent,
        // The removal is from the ability's own object (runtime boundary);
        // the destination is the action's target operand.
        LocalEffectNode::MoveCounters(node) => &node.destination,
        LocalEffectNode::TapPermanents(node) => &node.targets,
        LocalEffectNode::UntapRestriction(node) => &node.objects,
        LocalEffectNode::PermanentKeywordAction(node) => &node.targets,
        LocalEffectNode::ControlChange(node) => &node.objects,
        _ => return None,
    };
    match operand {
        PermanentOperandNode::Targets(targets) => Some(targets),
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn lower_ordinary_permanent_subject(
    cursor: &mut OracleTokenCursor<'_>,
    operand: &PermanentOperandNode,
    effect_index: usize,
    exclude_first_source: bool,
    target_base_ids: &[&str],
    used_base_ids: &mut Vec<&'static str>,
    decisions: &mut Vec<Value>,
    bindings: &mut Vec<(TargetBinding, String)>,
) -> MachineResult<Vec<Value>> {
    let PermanentOperandNode::Targets(targets) = operand else {
        return Ok(vec![match operand {
            PermanentOperandNode::BoundReference(reference) => {
                lower_bound_permanent_reference(cursor, *reference, bindings)?
            }
            _ => lower_nontargeted_permanent_operand(operand)
                .map_err(|error| cursor.fail(error.expectation()))?,
        }]);
    };
    // `another target X` after an earlier target slot carries the typed
    // `otherThanEarlierTargets` relation from the target-list child; the card
    // composition resolves it against the declaration (`distinctFrom`).
    let mut objects = Vec::new();
    for (slot_index, slot) in targets.slots.iter().enumerate() {
        cursor.enter("lower.permanent-subject.target-slot");
        cursor.require(
            selector_guarantees_permanent(&slot.selector),
            "target slots guaranteed to denote battlefield permanents",
        )?;
        // The cardinality `N` / `up to N` is the shared numeric atom read by
        // the target-list child; its bound only selects the reference arity.
        // A typed count (`X target`, `any number of target`) is plural and
        // replaces the literal envelope below.
        let plural = slot
            .count
            .as_ref()
            .map_or(slot.cardinality.maximum > 1, TargetCount::is_plural);
        let (base, mut candidates) = lower_target_slot_candidates(slot)
            .ok_or_else(|| cursor.fail("a supported permanent target candidate query"))?;
        if slot_index == 0 && exclude_first_source {
            crate::oracle::canonical::automata::mark_other_target_slot(&mut candidates);
        }
        let ordinal = used_base_ids
            .iter()
            .filter(|candidate| **candidate == base)
            .count();
        let total = target_base_ids
            .iter()
            .filter(|candidate| **candidate == base)
            .count();
        let id = super::effect_sequence::target_id_for_ordinal(base, ordinal, total);
        used_base_ids.push(base);
        let mut decision = target_decision(
            &id,
            candidates,
            i64::from(slot.cardinality.minimum),
            i64::from(slot.cardinality.maximum),
        );
        if let Some(count) = &slot.count {
            cursor.enter("lower.permanent-subject.target-count");
            cursor.require(
                apply_target_count_to_decision(&mut decision, count).is_some(),
                "a declared target count the target decision can bound",
            )?;
        }
        decisions.push(decision);
        if plural {
            // A target collection has no singular anaphor binding (the
            // parser never names a plural slot with `it` / `that creature`);
            // its binding serves only a `TargetSet` reference (`those
            // creatures`), which lowers to the same collection.
            bindings.push((
                target_set_binding_key(TargetBinding {
                    effect_index,
                    slot_index,
                }),
                id.clone(),
            ));
            objects.push(json!({"kind":"chosenTargets","id":id}));
            continue;
        }
        bindings.push((
            TargetBinding {
                effect_index,
                slot_index,
            },
            id.clone(),
        ));
        objects.push(chosen_target(&id));
    }
    Ok(objects)
}
