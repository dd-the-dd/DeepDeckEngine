//! One subject-lowering boundary for ordinary permanent actions.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_bound_permanent_reference(
    cursor: &OracleTokenCursor<'_>,
    reference: PermanentEffectReference,
    bindings: &[(TargetBinding, String)],
) -> MachineResult<Value> {
    match reference {
        PermanentEffectReference::Source(LocalObjectReference::SourceSelf) => Ok(self_ref()),
        PermanentEffectReference::Observed(TypedObservedEventObject::EnteringPermanent) => {
            Ok(json!({"kind":"observedEventObject","event":"permanentEntered"}))
        }
        PermanentEffectReference::Observed(TypedObservedEventObject::CreatedTokens) => {
            Ok(json!({"kind":"boundObjects","binding":CREATED_TOKENS_BINDING}))
        }
        PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield) => {
            Ok(json!({"kind":"singleBoundObject","binding":MOVED_ONTO_BATTLEFIELD_BINDING}))
        }
        PermanentEffectReference::Target(target) => bindings
            .iter()
            .find_map(|(binding, id)| (*binding == target).then(|| chosen_target(id)))
            .ok_or_else(|| {
                cursor.fail("the previously declared permanent target slot in this lexical scope")
            }),
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
        LocalEffectNode::TapPermanents(node) => &node.targets,
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
    cursor.require(
        !targets.slots.iter().enumerate().any(|(index, slot)| {
            slot.exclude_source
                && (index > 0
                    || decisions
                        .iter()
                        .any(|decision| decision["kind"].as_str() == Some("chooseTargets")))
        }),
        "an explicit distinct-target relation for another after an earlier target declaration",
    )?;
    let mut objects = Vec::new();
    for (slot_index, slot) in targets.slots.iter().enumerate() {
        cursor.enter("lower.permanent-subject.target-slot");
        cursor.require(
            selector_guarantees_permanent(&slot.selector),
            "target slots guaranteed to denote battlefield permanents",
        )?;
        // The cardinality `N` / `up to N` is the shared numeric atom read by
        // the target-list child; its bound only selects the reference arity.
        let plural = slot.cardinality.maximum > 1;
        let (base, mut candidates) = lower_target_slot_candidates(slot)
            .ok_or_else(|| cursor.fail("a supported permanent target candidate query"))?;
        if slot_index == 0 && exclude_first_source {
            candidates["excludeSource"] = json!(true);
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
        decisions.push(target_decision(
            &id,
            candidates,
            i64::from(slot.cardinality.minimum),
            i64::from(slot.cardinality.maximum),
        ));
        if plural {
            // A target collection has no singular anaphor binding; a later
            // `it` / `that creature` keeps its explicit missing-slot failure.
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
