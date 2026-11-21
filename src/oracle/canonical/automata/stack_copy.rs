//! `copy TARGET` over a stack object (CR 707.10): the copy is created on the
//! stack under the ability controller's control. The target slot and its
//! spell/stack-item predicate are the same children used by `counter`.
//!
//! `You may choose new targets for the copy.` (CR 707.10c) is not an action:
//! it amends the copy instruction of the preceding sentence and is validated
//! against that typed antecedent by the enclosing sequence.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    TargetCardinality, TargetListNode, recognize_target_list, stack_domain_selector_supported,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CopyStackObjectEffectNode {
    pub target: TargetListNode,
    /// Set only by the following new-targets sentence (CR 707.10c).
    pub may_choose_new_targets: bool,
}

pub(in crate::oracle::canonical) fn copy_stack_object_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("copy"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
}

pub(in crate::oracle::canonical) fn recognize_copy_stack_object_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CopyStackObjectEffectNode> {
    cursor.enter("effect.copy-stack-object.action");
    cursor.consume("copy")?;
    cursor.enter("effect.copy-stack-object.target");
    let target = recognize_target_list(cursor)?;
    cursor.enter("effect.copy-stack-object.target-domain");
    let [slot] = target.slots.as_slice() else {
        return Err(cursor.fail("exactly one target slot for a copy effect"));
    };
    cursor.require(
        slot.cardinality == TargetCardinality::EXACTLY_ONE,
        "exactly one required stack-object target",
    )?;
    cursor.require(
        stack_domain_selector_supported(&slot.selector),
        "a spell or stack-item selector",
    )?;
    Ok(CopyStackObjectEffectNode {
        target,
        may_choose_new_targets: false,
    })
}

/// True at `you may choose new targets for`.
pub(in crate::oracle::canonical) fn copy_new_targets_amendment_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ["you", "may", "choose", "new", "targets", "for"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Reads `you may choose new targets for the copy.` and applies it to the copy
/// instruction it follows. The noun is singular because exactly one copy is
/// created; the sentence terminator stays with the enclosing sequence.
pub(in crate::oracle::canonical) fn recognize_copy_new_targets_amendment(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&mut CopyStackObjectEffectNode>,
) -> MachineResult<()> {
    cursor.enter("effect.copy-stack-object.new-targets");
    let antecedent = antecedent.ok_or_else(|| {
        cursor.fail("a copy instruction in the preceding sentence for the new-targets permission")
    })?;
    cursor.require(
        !antecedent.may_choose_new_targets,
        "one new-targets permission for the copy",
    )?;
    cursor.consume_phrase(&["you", "may", "choose", "new", "targets", "for", "the"])?;
    cursor.enter("effect.copy-stack-object.new-targets.object");
    cursor.consume("copy")?;
    antecedent.may_choose_new_targets = true;
    Ok(())
}
