//! `copy TARGET` over a stack object (CR 707.10): the copy is created on the
//! stack under the ability controller's control. The target slot and its
//! spell/stack-item predicate are the same children used by `counter`.
//!
//! `You may choose new targets for the copy.` (CR 707.10c) is not an action:
//! it amends the copy instruction of the preceding sentence and is validated
//! against that typed antecedent by the enclosing sequence.
//!
//! ```text
//! Copy     := "copy" OBJECT [COUNT] [EXCEPT] ["and" NEW-TARGETS]
//! OBJECT   := TARGET-LIST                       (declared stack object)
//!           | "that spell" | "it"               (the spell a cast record binds)
//!           | "this spell" | "it"               (the source spell itself)
//! COUNT    := "twice" | "thrice" | N "times"    (literal copies)
//!           | "X" "times" | "for each" COUNTABLE (the shared quantity slot,
//!                                                evaluated at resolution)
//! ```
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
    /// `copy this spell` while the spell itself resolves (CR 707.10): the
    /// resolving spell, not a cast record. Exclusive with `event_spell`.
    pub resolving_spell: bool,
    /// Set only by the following new-targets sentence (CR 707.10c).
    pub may_choose_new_targets: bool,
    /// `twice`, `three times`: how many copies one instruction creates,
    /// each an ordinary copy of the same object (CR 707.10).
    pub count: i64,
    /// `X times`, `for each COUNTABLE`: a variable number of copies, the
    /// shared numeric expression evaluated as the instruction resolves.
    /// `count` is then unused.
    pub variable_count: Option<super::NumericExpression>,
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
        && (copy_new_targets_amendment_starts(&probe) || shared_subject_new_targets_starts(&probe)))
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
    plural: bool,
) -> MachineResult<TokenCopyExceptionsNode> {
    if cursor.peek() == Some(",")
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("except"))
    {
        cursor.enter("effect.copy-stack-object.exceptions");
        return recognize_copy_exceptions(cursor, plural);
    }
    Ok(TokenCopyExceptionsNode::default())
}

/// The largest literal number of copies one copy instruction creates.
const MAXIMUM_STACK_COPIES: i64 = 10;

/// The number of copies one instruction creates.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CopyCount {
    Literal(i64),
    Variable(super::NumericExpression),
}

/// `X times` / `for each COUNTABLE` after the copied object: the shared
/// quantity slot (CR 107.3, 608.2h). Other counts are literal.
fn recognize_copy_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<CopyCount> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("x"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("times"))
    {
        cursor.enter("effect.copy-stack-object.count.variable");
        cursor.advance_to(cursor.position() + 2)?;
        return Ok(CopyCount::Variable(super::NumericExpression::Atom(
            super::NumericAtom::VariableX,
        )));
    }
    if super::for_each_count_starts(cursor) {
        cursor.enter("effect.copy-stack-object.count.for-each");
        return super::recognize_for_each_count(cursor).map(CopyCount::Variable);
    }
    recognize_literal_copy_count(cursor).map(CopyCount::Literal)
}

impl CopyCount {
    fn split(self) -> (i64, Option<super::NumericExpression>) {
        match self {
            Self::Literal(count) => (count, None),
            // The literal slot is unused once the count is variable.
            Self::Variable(expression) => (1, Some(expression)),
        }
    }

    fn plural(&self) -> bool {
        !matches!(self, Self::Literal(1))
    }
}

/// `twice` / `thrice` / `N times` after the copied object; absent is one
/// copy.
fn recognize_literal_copy_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
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

/// The spell the ability is written on, as a stack object (CR 707.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceSpellRole {
    /// `When you cast this spell, copy it|this spell`: the cast record of the
    /// source spell binds it (CR 601.2i), as the event spell of a cast.
    CastRecord,
    /// `copy this spell` in the spell's own effect: the resolving spell.
    Resolving,
}

/// `this spell`, or `it` without any lexical object antecedent in the
/// source spell's own cast trigger. A permanent antecedent keeps `it`.
fn source_spell_object_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(usize, SourceSpellRole)> {
    let word = |delta: usize, expected: &str| {
        cursor
            .peek_ahead(offset + delta)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let environment = cursor.binding_environment();
    let cast_record = environment.is_some_and(|environment| environment.frame.source_spell_cast);
    if word(0, "this") && word(1, "spell") {
        return Some((
            2,
            if cast_record {
                SourceSpellRole::CastRecord
            } else {
                SourceSpellRole::Resolving
            },
        ));
    }
    (word(0, "it")
        && cast_record
        && environment.is_some_and(|environment| {
            environment.object.is_none() && environment.frame.event_object.is_none()
        }))
    .then_some((1, SourceSpellRole::CastRecord))
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
            || event_spell_object_at(cursor, 1).is_some()
            || source_spell_object_at(cursor, 1).is_some())
}

/// The instruction once its object is known: count, exceptions and a
/// coordinated new-targets permission.
fn recognize_copy_tail(
    cursor: &mut OracleTokenCursor<'_>,
    target: TargetListNode,
    event_spell: bool,
    resolving_spell: bool,
    controller: Option<super::ObjectControllerScope>,
) -> MachineResult<CopyStackObjectEffectNode> {
    let count = recognize_copy_count(cursor)?;
    let exceptions = recognize_stack_copy_exceptions(cursor, count.plural())?;
    let (count, variable_count) = count.split();
    let mut node = CopyStackObjectEffectNode {
        target,
        event_spell,
        resolving_spell,
        may_choose_new_targets: false,
        count,
        variable_count,
        exceptions,
        controller,
    };
    recognize_coordinated_new_targets(cursor, &mut node)?;
    Ok(node)
}

pub(in crate::oracle::canonical) fn recognize_copy_stack_object_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CopyStackObjectEffectNode> {
    cursor.enter("effect.copy-stack-object.action");
    cursor.consume("copy")?;
    if let Some(width) = event_spell_object_at(cursor, 0) {
        cursor.enter("effect.copy-stack-object.event-spell");
        cursor.advance_to(cursor.position() + width)?;
        return recognize_copy_tail(
            cursor,
            TargetListNode { slots: Vec::new() },
            true,
            false,
            None,
        );
    }
    if let Some((width, role)) = source_spell_object_at(cursor, 0) {
        cursor.enter("effect.copy-stack-object.source-spell");
        cursor.advance_to(cursor.position() + width)?;
        return recognize_copy_tail(
            cursor,
            TargetListNode { slots: Vec::new() },
            role == SourceSpellRole::CastRecord,
            role == SourceSpellRole::Resolving,
            None,
        );
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
    recognize_copy_tail(cursor, target, false, false, controller)
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
        | super::EffectProgramNode::CreatedReplacement { .. }
        | super::EffectProgramNode::OncePerTurn { .. } => None,
    }
}

fn words_at(cursor: &OracleTokenCursor<'_>, offset: usize, words: &[&str]) -> bool {
    words.iter().enumerate().all(|(delta, word)| {
        cursor
            .peek_ahead(offset + delta)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    })
}

/// The width of `choose new targets for` / `choose a new target for` at
/// `offset`: the same permission (CR 707.10c), singular when the copied
/// spell has one target.
fn choose_new_targets_width(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    if words_at(cursor, offset, &["choose", "new", "targets", "for"]) {
        Some(4)
    } else if words_at(cursor, offset, &["choose", "a", "new", "target", "for"]) {
        Some(5)
    } else {
        None
    }
}

/// True at `you may choose new targets for` / `you may choose a new target
/// for`.
pub(in crate::oracle::canonical) fn copy_new_targets_amendment_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    words_at(cursor, 0, &["you", "may"]) && choose_new_targets_width(cursor, 2).is_some()
}

/// `may choose a new target for ...` after `and`: the permission shares the
/// copying player as its subject (`they may copy this spell and may choose
/// a new target for that copy`).
fn shared_subject_new_targets_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    words_at(cursor, 0, &["may"]) && choose_new_targets_width(cursor, 1).is_some()
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
    if words_at(cursor, 0, &["you"]) {
        cursor.consume("you")?;
    }
    cursor.consume("may")?;
    let width = cursor.require_value(
        choose_new_targets_width(cursor, 0),
        "`choose new targets for` or `choose a new target for`",
    )?;
    cursor.advance_to(cursor.position() + width)?;
    cursor.enter("effect.copy-stack-object.new-targets.object");
    let determiner = if words_at(cursor, 0, &["that"]) || words_at(cursor, 0, &["those"]) {
        cursor.peek().unwrap_or_default().to_owned()
    } else {
        "the".to_string()
    };
    cursor.consume(&determiner)?;
    let plural = antecedent.count != 1 || antecedent.variable_count.is_some();
    cursor.consume(if plural { "copies" } else { "copy" })?;
    antecedent.may_choose_new_targets = true;
    Ok(())
}
