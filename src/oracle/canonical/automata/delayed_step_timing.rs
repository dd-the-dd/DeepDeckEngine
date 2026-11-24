use super::super::{MachineResult, OracleTokenCursor};
use super::DelayedTurnStep;

/// Whose turn supplies the step that wakes a delayed trigger (CR 603.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DelayedStepTurnNode {
    /// `the next STEP`: the next such step of any player's turn.
    AnyTurn,
    /// `your next STEP`: the next such step of the controller's turn.
    ControllerTurn,
    /// `the next turn's STEP`: that step of the turn after this one, never a
    /// later step of the current turn.
    NextTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelayedStepTimingNode {
    pub step: DelayedTurnStep,
    pub turn: DelayedStepTurnNode,
}

/// A program range whose actions are not performed now: resolving creates a
/// rule that performs them later, a delayed triggered ability at the stated
/// step or event (CR 603.7a) or a replacement (CR 614.1a). The sequence
/// parent owns the range; the rule node owns timing, event and lifetime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelayedFrameNode {
    pub first_effect: usize,
    pub end_effect: usize,
    pub rule: super::EffectCreatedRuleNode,
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
    let phrase = |words: &[&str]| {
        words.iter().enumerate().all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
    };
    phrase(&["at", "the", "beginning", "of"]) || phrase(&["at", "end", "of", "combat"])
}

/// Reads `at the beginning of {the|your} next [turn's] {end step|upkeep}`.
pub(in crate::oracle::canonical) fn recognize_delayed_step_timing(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DelayedStepTimingNode> {
    cursor.enter("delayed-step.timing.introducer");
    // `at end of combat`: the next end of combat step, this combat's when
    // one is in progress (CR 511.2, 603.7a).
    if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("end"))
    {
        cursor.consume_phrase(&["at", "end", "of", "combat"])?;
        return Ok(DelayedStepTimingNode {
            step: DelayedTurnStep::EndOfCombat,
            turn: DelayedStepTurnNode::AnyTurn,
        });
    }
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
    // `the next turn's STEP`: the step of the following turn (CR 500.1).
    let turn = if turn == DelayedStepTurnNode::AnyTurn
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("turn's") || raw == "turn\u{2019}s")
    {
        cursor.enter("delayed-step.timing.next-turn");
        let possessive = cursor.peek().expect("checked possessive turn").to_string();
        cursor.consume(&possessive)?;
        DelayedStepTurnNode::NextTurn
    } else {
        turn
    };
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
