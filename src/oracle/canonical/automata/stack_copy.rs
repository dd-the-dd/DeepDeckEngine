//! `copy TARGET` over a stack object (CR 707.10): the copy is created on the
//! stack under the ability controller's control. The target slot and its
//! spell/stack-item predicate are the same children used by `counter`.
//!
//! `You may choose new targets for the copy.` (CR 707.10c) is not an action:
//! it amends the copy instruction of the preceding sentence and is validated
//! against that typed antecedent by the enclosing sequence.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    TargetCardinality, TargetListNode, TokenCopyExceptionsNode, recognize_copy_exceptions,
    recognize_target_list, stack_domain_selector_supported,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CopyStackObjectEffectNode {
    /// The declared stack-object target; empty when `event_spell` is set.
    pub target: TargetListNode,
    /// `copy that spell` / `copy it`: the spell the triggering cast record
    /// binds (CR 707.10), like `counter that spell`.
    pub event_spell: bool,
    /// Set only by the following new-targets sentence (CR 707.10c).
    pub may_choose_new_targets: bool,
    /// `twice`, `three times`: how many copies one instruction creates,
    /// each an ordinary copy of the same object (CR 707.10).
    pub count: i64,
    /// `, except the copy isn't legendary` (CR 707.9b, 707.10): the
    /// copiable-value exceptions of every copy, read by the shared reader.
    pub exceptions: TokenCopyExceptionsNode,
    /// `target triggered ability you control`: the controller relation of
    /// a stack-item target (CR 113.8, 115.1).
    pub controller: Option<super::ObjectControllerScope>,
}

/// `you control` after a stack-ability target: the one relation the copy
/// parent admits for a stack item. Any other relation stays unread.
fn recognize_stack_item_controller(
    cursor: &mut OracleTokenCursor<'_>,
    target: &TargetListNode,
) -> MachineResult<Option<super::ObjectControllerScope>> {
    let stack_item = matches!(
        target.slots.as_slice(),
        [slot] if matches!(slot.selector, super::TargetSelector::StackItem(_))
    );
    if !(stack_item
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("control")))
    {
        return Ok(None);
    }
    cursor.enter("effect.copy-stack-object.controller");
    cursor.consume_phrase(&["you", "control"])?;
    Ok(Some(super::ObjectControllerScope::AbilityController))
}

/// `and you may choose new targets for the copy` coordinated with the copy
/// instruction itself (CR 707.10c).
fn recognize_coordinated_new_targets(
    cursor: &mut OracleTokenCursor<'_>,
    node: &mut CopyStackObjectEffectNode,
) -> MachineResult<()> {
    let mut probe = cursor.clone();
    if !(probe
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
        && probe.consume("and").is_ok()
        && copy_new_targets_amendment_starts(&probe))
    {
        return Ok(());
    }
    cursor.enter("effect.copy-stack-object.coordinated-new-targets");
    cursor.consume("and")?;
    recognize_copy_new_targets_amendment(cursor, Some(node))
}

/// `, except ...` after the copied object and its count.
fn recognize_stack_copy_exceptions(
    cursor: &mut OracleTokenCursor<'_>,
    count: i64,
) -> MachineResult<TokenCopyExceptionsNode> {
    if cursor.peek() == Some(",")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("except"))
    {
        cursor.enter("effect.copy-stack-object.exceptions");
        return recognize_copy_exceptions(cursor, count > 1);
    }
    Ok(TokenCopyExceptionsNode::default())
}

/// The largest literal number of copies one copy instruction creates.
const MAXIMUM_STACK_COPIES: i64 = 10;

/// `twice` / `thrice` / `N times` after the copied object; absent is one
/// copy. Only a literal count is read here: a variable count (`X times`,
/// `for each ...`) has no repetition program on this primitive yet.
fn recognize_copy_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let count = if word(0, "twice") {
        Some((1, 2))
    } else if word(0, "thrice") {
        Some((1, 3))
    } else if word(1, "times") {
        cursor
            .peek()
            .and_then(super::super::parse_number_word)
            .map(|count| (2, count))
    } else {
        None
    };
    let Some((width, count)) = count else {
        return Ok(1);
    };
    cursor.enter("effect.copy-stack-object.count");
    cursor.require(
        (2..=MAXIMUM_STACK_COPIES).contains(&count),
        "a literal number of copies within the copy bound",
    )?;
    cursor.advance_to(cursor.position() + width)?;
    Ok(count)
}

/// `that spell` / `it` naming the spell a cast record binds. An object
/// antecedent keeps priority for `it` and is never reinterpreted.
fn event_spell_object_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    let environment = cursor.binding_environment()?;
    if !environment.frame.event_spell {
        return None;
    }
    let word = |delta: usize, expected: &str| {
        cursor
            .peek_ahead(offset + delta)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(0, "that") && word(1, "spell") {
        Some(2)
    } else if word(0, "it") && environment.object.is_none() {
        Some(1)
    } else {
        None
    }
}

pub(in crate::oracle::canonical) fn copy_stack_object_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("copy"))
        && (cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
            || event_spell_object_at(cursor, 1).is_some())
}

pub(in crate::oracle::canonical) fn recognize_copy_stack_object_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CopyStackObjectEffectNode> {
    cursor.enter("effect.copy-stack-object.action");
    cursor.consume("copy")?;
    if let Some(width) = event_spell_object_at(cursor, 0) {
        cursor.enter("effect.copy-stack-object.event-spell");
        cursor.advance_to(cursor.position() + width)?;
        let count = recognize_copy_count(cursor)?;
        let exceptions = recognize_stack_copy_exceptions(cursor, count)?;
        let mut node = CopyStackObjectEffectNode {
            target: TargetListNode { slots: Vec::new() },
            event_spell: true,
            may_choose_new_targets: false,
            count,
            exceptions,
            controller: None,
        };
        recognize_coordinated_new_targets(cursor, &mut node)?;
        return Ok(node);
    }
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
    let controller = recognize_stack_item_controller(cursor, &target)?;
    let count = recognize_copy_count(cursor)?;
    let exceptions = recognize_stack_copy_exceptions(cursor, count)?;
    let mut node = CopyStackObjectEffectNode {
        target,
        event_spell: false,
        may_choose_new_targets: false,
        count,
        exceptions,
        controller,
    };
    recognize_coordinated_new_targets(cursor, &mut node)?;
    Ok(node)
}

/// The copy instruction a following new-targets sentence amends: the last
/// action written before it, through the program wrappers that only gate
/// that action (`you may copy it`, `If you do, copy that spell`, CR 608.2c).
pub(in crate::oracle::canonical) fn last_copy_instruction_mut(
    effects: &mut [super::LocalEffectNode],
) -> Option<&mut CopyStackObjectEffectNode> {
    match effects.last_mut()? {
        super::LocalEffectNode::CopyStackObject(copy) => Some(copy),
        super::LocalEffectNode::OptionalSequence(sequence) => {
            last_copy_instruction_mut(&mut sequence.effects)
        }
        _ => None,
    }
}

/// [`last_copy_instruction_mut`] over a program whose last written sentence
/// holds the copy instruction.
pub(in crate::oracle::canonical) fn last_program_copy_instruction_mut(
    program: &mut super::EffectProgramNode,
) -> Option<&mut CopyStackObjectEffectNode> {
    match program {
        super::EffectProgramNode::Sequence(sequence) => {
            last_copy_instruction_mut(&mut sequence.effects)
        }
        super::EffectProgramNode::Optional { body, .. } => last_program_copy_instruction_mut(body),
        // `If you don't, ...` is the last sentence of its block; only a
        // block without one ends with its body.
        super::EffectProgramNode::Outcome {
            body,
            otherwise: None,
            ..
        } => last_program_copy_instruction_mut(body),
        super::EffectProgramNode::Steps(steps) => {
            last_program_copy_instruction_mut(steps.last_mut()?)
        }
        super::EffectProgramNode::Outcome { .. }
        | super::EffectProgramNode::Alternatives { .. }
        | super::EffectProgramNode::Offer { .. }
        | super::EffectProgramNode::CreatedReplacement { .. } => None,
    }
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

/// Reads `you may choose new targets for the copy|copies.` and applies it to
/// the copy instruction it follows. The noun agrees with the number of copies
/// the instruction creates; the sentence terminator stays with the enclosing
/// sequence.
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
    cursor.consume(if antecedent.count == 1 {
        "copy"
    } else {
        "copies"
    })?;
    antecedent.may_choose_new_targets = true;
    Ok(())
}
