use super::super::{MachineResult, OracleTokenCursor};
use super::DelayedTurnStep;

/// Whose turn supplies the step that wakes a delayed trigger (CR 603.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DelayedStepTurnNode {
    /// `the next STEP`: the next such step of any player's turn.
    AnyTurn,
    /// `your next STEP`: the next such step of the controller's turn.
    ControllerTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelayedStepTimingNode {
    pub step: DelayedTurnStep,
    pub turn: DelayedStepTurnNode,
}

/// A program range whose actions are not performed now: resolving creates a
/// delayed triggered ability that performs them at the stated step
/// (CR 603.7a). The sequence parent owns the range; this node owns timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelayedFrameNode {
    pub first_effect: usize,
    pub end_effect: usize,
    pub timing: DelayedStepTimingNode,
}

impl DelayedFrameNode {
    pub(in crate::oracle::canonical) fn shifted(self, offset: usize) -> Self {
        Self {
            first_effect: self.first_effect + offset,
            end_effect: self.end_effect + offset,
            ..self
        }
    }
}

/// Whether the cursor stands on `at the beginning of`, the introducer of a
/// delayed step timing inside an effect program.
pub(in crate::oracle::canonical) fn delayed_step_timing_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ["at", "the", "beginning", "of"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Reads `at the beginning of {the|your} next {end step|upkeep}`.
pub(in crate::oracle::canonical) fn recognize_delayed_step_timing(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DelayedStepTimingNode> {
    cursor.enter("delayed-step.timing.introducer");
    cursor.consume_phrase(&["at", "the", "beginning", "of"])?;
    cursor.enter("delayed-step.timing.turn");
    let turn = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("the") => {
            cursor.consume("the")?;
            DelayedStepTurnNode::AnyTurn
        }
        Some("your") => {
            cursor.consume("your")?;
            DelayedStepTurnNode::ControllerTurn
        }
        _ => return Err(cursor.fail("`the next` or `your next`")),
    };
    cursor.enter("delayed-step.timing.next");
    cursor.consume("next")?;
    cursor.enter("delayed-step.timing.step");
    let step = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("upkeep") => {
            cursor.consume("upkeep")?;
            DelayedTurnStep::Upkeep
        }
        Some("end") => {
            cursor.consume_phrase(&["end", "step"])?;
            DelayedTurnStep::EndStep
        }
        _ => return Err(cursor.fail("`upkeep` or `end step`")),
    };
    Ok(DelayedStepTimingNode { step, turn })
}

#[cfg(test)]
#[path = "delayed_step_timing_tests.rs"]
mod tests;
