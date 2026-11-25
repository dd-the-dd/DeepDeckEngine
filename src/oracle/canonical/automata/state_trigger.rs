//! State triggers (CR 603.8): `When CONDITION, EFFECT.`
//!
//! ```text
//! StateTrigger := "when" CONDITION        -- the one condition atom reader
//! ```
//!
//! A state-triggered ability triggers when a game state is true, not when
//! an event occurs. Its condition is the shared condition language read in
//! the static scope (no cast record, previous action or declared target
//! exists; `it` and `this PERMANENT` denote the ability's source), exactly
//! as a `while` event-time condition or an `as long as` static ability. The
//! enclosing event clause owns the trigger word and the boundary; this child
//! reads only the condition and is tried after every event alternative, so a
//! known occurrence is never reread as a state.
//!
//! A state trigger is introduced by `when` (it triggers once and does not
//! trigger again while it is pending, CR 603.8). A condition about what
//! happened earlier (an event history) is not a state the engine rechecks
//! and is refused, as is any event modifier on a state.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ConditionAtom, ConditionContext, ConditionFamily, ConditionScope, EffectRecognitionContext,
    LocalObjectReference, SentenceConditionContext, SentenceConditionNode, condition_any,
    recognize_boolean_condition_list_before_separator, recognize_condition_atom,
};

/// Whether the trigger word just before the cursor is `when`.
fn introduced_by_when(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .position()
        .checked_sub(1)
        .and_then(|index| cursor.token_slice(index, index + 1).first())
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("when"))
}

/// Reads the condition of a state trigger after `when`. The enclosing event
/// clause consumes the comma that opens the effect program.
pub(in crate::oracle::canonical) fn recognize_state_trigger_condition(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SentenceConditionNode> {
    cursor.enter("event.state-trigger.introducer");
    cursor.require(
        introduced_by_when(cursor),
        "`when` introducing a state trigger (CR 603.8)",
    )?;
    let effects = EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf);
    let program = SentenceConditionContext {
        name_links,
        name_failure,
        effects: &effects,
        card_target: None,
        card_focus: false,
        departed_targets: &[],
    };
    let context = ConditionContext::program(ConditionScope::Static, &program);
    cursor.enter("event.state-trigger.condition");
    let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_condition_atom(cursor, &context)
    })?;
    cursor.enter("event.state-trigger.state");
    cursor.require(
        !condition_any(&condition, &|atom: &ConditionAtom| {
            atom.family() == ConditionFamily::EventHistory
        }),
        "a game state, not an event history (CR 603.8)",
    )?;
    Ok(condition)
}

#[cfg(test)]
#[path = "state_trigger_tests.rs"]
mod tests;
