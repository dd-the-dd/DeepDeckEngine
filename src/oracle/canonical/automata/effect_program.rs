//! Program composition is independent of the enclosing ability and action.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum EffectProgramNode {
    Sequence(EffectSequenceNode),
    Optional {
        player: LocalPlayerReference,
        body: Box<EffectProgramNode>,
    },
    /// `you may COST. (If you do | If you don't | When you do), BODY`: the
    /// cost is paid at resolution and its outcome controls the body.
    OptionalCost {
        player: LocalPlayerReference,
        cost: ResolutionCostNode,
        outcome: CostOutcomeContinuation,
        body: Box<EffectProgramNode>,
    },
    /// `you may ACTION. (If you do | If you don't | When you do), BODY`: an
    /// ordinary instruction, not a cost. The outcome observes whether the
    /// instruction was actually performed (CR 608.2c, 603.12), not merely
    /// chosen. The action and the body are each bounded to one sentence.
    OptionalAction {
        player: LocalPlayerReference,
        action: Box<EffectProgramNode>,
        outcome: CostOutcomeContinuation,
        body: Box<EffectProgramNode>,
    },
    /// `ACTION. When you do, BODY.`: a mandatory instruction whose actual
    /// performance creates a reflexive triggered ability (CR 603.12). Both
    /// sentences are bounded; the body chooses its own targets.
    ReflexiveAction {
        action: Box<EffectProgramNode>,
        body: Box<EffectProgramNode>,
    },
    /// Independent sentence groups in written order. An outcome continuation
    /// conditions its own sentence only (CR 608.2c); later sentences are
    /// ordinary instructions. Steps are flat: no step is itself `Steps`.
    Steps(Vec<EffectProgramNode>),
}

impl EffectProgramNode {
    pub(in crate::oracle::canonical) fn inner_sequence(&self) -> &EffectSequenceNode {
        match self {
            Self::Sequence(sequence) => sequence,
            // The outcome sentence is the last written sentence of the block.
            Self::Optional { body, .. }
            | Self::OptionalCost { body, .. }
            | Self::OptionalAction { body, .. }
            | Self::ReflexiveAction { body, .. } => body.inner_sequence(),
            // The last written sentence group: trailing reminders and limits
            // read its final action. Lowering visits every step instead.
            Self::Steps(steps) => steps
                .last()
                .expect("a program has at least one step")
                .inner_sequence(),
        }
    }
    /// Program steps in written order; a single program is its own one step.
    pub(in crate::oracle::canonical) fn steps(&self) -> &[EffectProgramNode] {
        match self {
            Self::Steps(steps) => steps,
            other => std::slice::from_ref(other),
        }
    }
    pub(in crate::oracle::canonical) fn action_count(&self) -> usize {
        match self {
            Self::Sequence(sequence) => sequence.effects.len(),
            Self::Optional { body, .. } | Self::OptionalCost { body, .. } => body.action_count(),
            Self::OptionalAction { action, body, .. } | Self::ReflexiveAction { action, body } => {
                action.action_count() + body.action_count()
            }
            Self::Steps(steps) => steps.iter().map(Self::action_count).sum(),
        }
    }
}

/// The first bounded optional program owns one complete Oracle sentence.
/// Its `and`/`then` connectors belong to the shared inner effect sequence.
/// An optional cost owns its outcome sentence; any other sentence is an
/// independent program step before or after that block.
pub(in crate::oracle::canonical) fn recognize_effect_program_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    if !optional_program_starts(cursor) {
        let sequence = cursor.attempt(|attempt| {
            recognize_effect_sequence_with_context(attempt, name_links, name_failure, context)
        });
        let failure = match sequence {
            Ok(sequence) => return Ok(EffectProgramNode::Sequence(sequence)),
            Err(failure) => failure,
        };
        // A mandatory first sentence may be the instruction of a reflexive
        // `When you do,` outcome (CR 603.12).
        let failure = if reflexive_outcome_follows_first_sentence(cursor) {
            match cursor.attempt(|attempt| {
                recognize_reflexive_action_block(attempt, name_links, name_failure, context)
            }) {
                Ok(block) => {
                    return complete_outcome_block(
                        cursor,
                        block,
                        name_links,
                        name_failure,
                        context,
                    );
                }
                // Report the reading that progressed furthest.
                Err(block_failure) if block_failure.token_index > failure.token_index => {
                    block_failure
                }
                Err(_) => failure,
            }
        } else {
            failure
        };
        // A later sentence may open an optional-cost block. The leading
        // sentences are then an ordinary, separately bounded program step.
        let Some(block_start) = later_optional_outcome_block(cursor) else {
            return Err(failure);
        };
        cursor.enter("program.steps.leading");
        let leading = cursor.bounded(block_start, |cursor| {
            recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
        })?;
        let mut steps = vec![EffectProgramNode::Sequence(leading)];
        push_program_steps(
            &mut steps,
            recognize_effect_program_with_context(cursor, name_links, name_failure, context)?,
        );
        return Ok(EffectProgramNode::Steps(steps));
    }
    // Preserve already supported action-local permissions (such as optional
    // draw followed by another sentence) and their exact historical IR.
    // This speculative typed child owns no semantic commit until complete.
    if let Ok(sequence) = cursor.attempt(|attempt| {
        recognize_effect_sequence_with_context(attempt, name_links, name_failure, context)
    }) {
        return Ok(EffectProgramNode::Sequence(sequence));
    }
    cursor.enter("program.optional.actor");
    let player = recognize_player_reference(cursor, None)?;
    cursor.enter("program.optional.permission");
    cursor.consume("may")?;
    if cost_outcome_follows_first_sentence(cursor) {
        // A payment head is first read as a resolution cost. Only when that
        // typed cost child rejects the sentence (for example a target or an
        // anaphor, which a cost cannot name) is it read as an instruction.
        let cost_failure = if optional_cost_head(cursor) {
            match cursor.attempt(|attempt| {
                recognize_optional_cost_block(attempt, player, name_links, name_failure, context)
            }) {
                Ok(block) => {
                    return complete_outcome_block(
                        cursor,
                        block,
                        name_links,
                        name_failure,
                        context,
                    );
                }
                Err(failure) => Some(failure),
            }
        } else {
            None
        };
        let block = cursor.attempt(|attempt| {
            recognize_optional_action_block(attempt, player, name_links, name_failure, context)
        });
        return match (block, cost_failure) {
            (Ok(block), _) => {
                complete_outcome_block(cursor, block, name_links, name_failure, context)
            }
            // Report the reading that progressed furthest; a tie keeps the cost.
            (Err(failure), Some(cost_failure))
                if failure.token_index > cost_failure.token_index =>
            {
                Err(failure)
            }
            (Err(_), Some(cost_failure)) => Err(cost_failure),
            (Err(failure), None) => Err(failure),
        };
    }
    // The permission owns its own sentence (and directly following reminders)
    // only. Without an outcome connector, a later sentence is an independent
    // program step that runs whether or not the permission was used (CR
    // 608.2c). A conditional or another permission inside the same sentence
    // has no enclosing frame here and stays unsupported. Reminder text and
    // quoted abilities are nested scopes (CR 207.2a, 113.1): their own `if`
    // or `may` is not a second frame of this sentence. The body reader still
    // reads and validates every reminder it owns.
    cursor.enter("program.optional.scope");
    let body_end = sentence_end_with_reminders(cursor);
    let mut offset = 0;
    let mut depth = 0_usize;
    let mut quoted = false;
    while cursor.position() + offset < body_end {
        let Some(raw) = cursor.peek_ahead(offset) else {
            break;
        };
        match raw {
            "\"" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0
            && !quoted
            && (raw.eq_ignore_ascii_case("if") || raw.eq_ignore_ascii_case("may"))
        {
            cursor.advance_to(cursor.position() + offset)?;
            return Err(cursor
                .fail("one optional sentence without an inner conditional or second permission"));
        }
        offset += 1;
    }
    cursor.enter("program.optional.body");
    // CR 118.12: an `unless` payer decides before this permission is used;
    // the optional wrapper cannot order that choice, so the body rejects it.
    let mut permission_context = context.clone();
    permission_context.unless_frames = UnlessFrameSupport::Unsupported;
    let body = cursor.bounded(body_end, |cursor| {
        recognize_effect_sequence_with_context(
            cursor,
            name_links,
            name_failure,
            &permission_context,
        )
    })?;
    cursor.enter("program.optional.complete");
    let optional = EffectProgramNode::Optional {
        player,
        body: Box::new(EffectProgramNode::Sequence(body)),
    };
    if cursor.peek().is_none() {
        return Ok(optional);
    }
    complete_outcome_block(cursor, optional, name_links, name_failure, context)
}

/// `COST. OUTCOME, BODY.` after `PLAYER may`.
fn recognize_optional_cost_block(
    cursor: &mut OracleTokenCursor<'_>,
    player: LocalPlayerReference,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.optional-cost.cost");
    let cost = recognize_resolution_cost(cursor)?;
    let outcome = recognize_cost_outcome_continuation(cursor)?;
    let body = recognize_outcome_body(
        cursor,
        "program.optional-cost.body",
        name_links,
        name_failure,
        context,
    )?;
    Ok(EffectProgramNode::OptionalCost {
        player,
        cost,
        outcome,
        body: Box::new(body),
    })
}

/// `ACTION. OUTCOME, BODY.` after `PLAYER may`. The action is the shared
/// effect sequence bounded to its own sentence; it never reads the outcome.
fn recognize_optional_action_block(
    cursor: &mut OracleTokenCursor<'_>,
    player: LocalPlayerReference,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.optional-action.action");
    let action_end = top_level_sentence_end(cursor)
        .ok_or_else(|| cursor.fail("one complete optional action sentence"))?;
    let action = cursor.bounded(action_end, |cursor| {
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
    })?;
    cursor.enter("program.optional-action.outcome");
    let outcome = recognize_outcome_connector(cursor)?;
    let body = recognize_outcome_body(
        cursor,
        "program.optional-action.body",
        name_links,
        name_failure,
        context,
    )?;
    Ok(EffectProgramNode::OptionalAction {
        player,
        action: Box::new(EffectProgramNode::Sequence(action)),
        outcome,
        body: Box::new(body),
    })
}

/// True when the current top-level sentence is followed by `When you do,`.
fn reflexive_outcome_follows_first_sentence(cursor: &OracleTokenCursor<'_>) -> bool {
    let Some(end) = top_level_sentence_end(cursor) else {
        return false;
    };
    let offset = end - cursor.position();
    ["when", "you", "do", ","]
        .iter()
        .enumerate()
        .all(|(index, word)| {
            cursor
                .peek_ahead(offset + index)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// `ACTION. When you do, BODY.` without a permission: the action is the
/// shared effect sequence bounded to its own sentence.
fn recognize_reflexive_action_block(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter("program.reflexive-action.action");
    let action_end = top_level_sentence_end(cursor)
        .ok_or_else(|| cursor.fail("one complete instruction sentence"))?;
    let action = cursor.bounded(action_end, |cursor| {
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
    })?;
    cursor.enter("program.reflexive-action.outcome");
    let outcome = recognize_outcome_connector(cursor)?;
    cursor.require(
        outcome == CostOutcomeContinuation::WhenPaid,
        "a reflexive `When you do,` outcome after a mandatory instruction",
    )?;
    let body = recognize_outcome_body(
        cursor,
        "program.reflexive-action.body",
        name_links,
        name_failure,
        context,
    )?;
    Ok(EffectProgramNode::ReflexiveAction {
        action: Box::new(EffectProgramNode::Sequence(action)),
        body: Box::new(body),
    })
}

/// The outcome governs its own sentence (and its reminder) only (CR 608.2c).
fn recognize_outcome_body(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    cursor.enter(state);
    let body_end = sentence_end_with_reminders(cursor);
    let body = cursor.bounded(body_end, |cursor| {
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
    })?;
    Ok(EffectProgramNode::Sequence(body))
}

/// An outcome block ends the program, yields to the enclosing trigger's
/// limit sentence, or is followed by independent program steps.
fn complete_outcome_block(
    cursor: &mut OracleTokenCursor<'_>,
    block: EffectProgramNode,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectProgramNode> {
    if cursor.peek().is_none()
        || (context.ability_restriction_owner == AbilityRestrictionOwner::Triggered
            && trigger_limit_starts(cursor))
    {
        cursor.enter("program.optional-outcome.complete");
        return Ok(block);
    }
    cursor.enter("program.steps.following");
    // A sentence-initial `Then` only restates the written order of the
    // independent steps (CR 608.2c); the following step owns its sentence.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("then"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw != "." && raw != ",")
    {
        cursor.enter("program.steps.then");
        cursor.consume("then")?;
    }
    let mut steps = vec![block];
    push_program_steps(
        &mut steps,
        recognize_effect_program_with_context(cursor, name_links, name_failure, context)?,
    );
    Ok(EffectProgramNode::Steps(steps))
}

/// End (exclusive, absolute) of the current top-level sentence, including
/// its period; `None` when no top-level period follows.
fn top_level_sentence_end(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let mut depth = 0usize;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => return Some(cursor.position() + offset + 1),
            _ => {}
        }
        offset += 1;
    }
    None
}

fn optional_program_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
}

fn push_program_steps(steps: &mut Vec<EffectProgramNode>, program: EffectProgramNode) {
    match program {
        EffectProgramNode::Steps(inner) => steps.extend(inner),
        other => steps.push(other),
    }
}

/// Absolute index of the first later top-level sentence that opens
/// `you may COST-OR-ACTION. (If you do | If you don't | When you do), ...`.
/// Lexical lookahead only; the block's children prove it afterwards.
pub(in crate::oracle::canonical) fn later_optional_outcome_block(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "." if depth == 0 => {
                let start = cursor.position() + offset + 1;
                let mut probe = cursor.clone();
                if probe.advance_to(start).is_ok() && optional_program_starts(&probe) {
                    let mut head = probe.clone();
                    if head.advance_to(start + 2).is_ok()
                        && cost_outcome_follows_first_sentence(&head)
                    {
                        return Some(start);
                    }
                }
            }
            _ => {}
        }
        offset += 1;
    }
    None
}

/// End (exclusive, absolute) of the current top-level sentence, extended over
/// parenthesized reminders that directly follow it (CR 207.2).
fn sentence_end_with_reminders(cursor: &OracleTokenCursor<'_>) -> usize {
    let mut depth = 0usize;
    let mut offset = 0;
    let mut end: Option<usize> = None;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if raw == "(" {
            depth += 1;
        } else if raw == ")" {
            depth = depth.saturating_sub(1);
            if depth == 0 && end.is_some() {
                end = Some(offset + 1);
            }
        } else if depth == 0 {
            if end.is_some() {
                break;
            }
            if raw == "." {
                end = Some(offset + 1);
            }
        }
        offset += 1;
    }
    cursor.position() + end.unwrap_or(offset)
}

#[cfg(test)]
#[path = "effect_program_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "effect_program_reminder_scope_tests.rs"]
mod reminder_scope_tests;
