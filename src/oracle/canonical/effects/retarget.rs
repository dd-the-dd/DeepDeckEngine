use super::super::*;

/// One `changeTargets` action and its stack-object target declaration. The
/// mode carries CR 115.7a/b/d; `with a single target` is a candidate
/// restriction of the declaration, never a resolution-time test.
pub(in crate::oracle::canonical) fn lower_retarget_action(
    cursor: &mut OracleTokenCursor<'_>,
    node: &RetargetActionNode,
    target_id: &str,
) -> MachineResult<(Value, Value)> {
    cursor.enter("lower.retarget.actor");
    cursor.require(
        node.player == LocalPlayerReference::Controller,
        "a controller-owned reassignment; other actors need a proved binding",
    )?;
    let [slot] = node.target.slots.as_slice() else {
        return Err(cursor.fail("one target stack-object slot"));
    };
    cursor.enter("lower.retarget.source-exclusion");
    cursor.require(
        !slot.exclude_source,
        "a stack target without an unproved source-exclusion relation",
    )?;
    let mut candidates = match &slot.selector {
        TargetSelector::StackItem(predicate) => {
            json!({"kind":"stackItems","where":lower_stack_item_filter(predicate)})
        }
        TargetSelector::Object(predicate) => lower_spell_target_candidates(predicate, None),
        TargetSelector::ControlledObject {
            predicate,
            controller: scope,
        } => lower_spell_target_candidates(predicate, Some(*scope)),
        _ => return Err(cursor.fail("a proved spell or stack-item selector")),
    };
    if node.single_target {
        candidates["singleTarget"] = json!(true);
    }
    let mut action = json!({
        "kind": "changeTargets",
        "object": chosen_target(target_id),
        "player": controller(),
        "optional": node.optional,
    });
    match node.mode {
        RetargetMode::ChooseNew => {}
        RetargetMode::ChangeAll => action["mode"] = json!("all"),
        RetargetMode::ChangeOne => action["mode"] = json!("one"),
    }
    if let Some(LocalObjectReference::SourceSelf) = node.forced {
        action["to"] = self_ref();
    }
    Ok((target_decision(target_id, candidates, 1, 1), action))
}
