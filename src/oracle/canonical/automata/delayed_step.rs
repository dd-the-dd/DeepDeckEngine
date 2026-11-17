use super::super::{MachineResult, OracleTokenCursor};
use super::LocalEffectNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DelayedTurnReference {
    NextTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DelayedTurnStep {
    Upkeep,
    EndStep,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelayedStepEffectNode {
    pub turn: DelayedTurnReference,
    pub step: DelayedTurnStep,
    pub effect: Box<LocalEffectNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DelayedStepState {
    Child,
    Relation,
    Beginning,
    Turn,
    Step,
    Complete,
}

/// Wraps one independently recognized child effect in a next-turn step
/// schedule. The caller chooses the child grammar; this machine owns only the
/// temporal suffix and therefore does not combine draw semantics with timing.
pub(in crate::oracle::canonical) fn recognize_delayed_step_effect<F>(
    cursor: &mut OracleTokenCursor<'_>,
    recognize_child: F,
) -> MachineResult<DelayedStepEffectNode>
where
    F: FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<LocalEffectNode>,
{
    let mut state = DelayedStepState::Child;
    let mut child_recognizer = Some(recognize_child);
    let mut effect = None;
    let mut step = None;
    loop {
        state = match state {
            DelayedStepState::Child => {
                cursor.enter("effect.delayed-step.child");
                let recognizer = child_recognizer
                    .take()
                    .ok_or_else(|| cursor.fail("one delayed child-effect recognizer"))?;
                effect = Some(recognizer(cursor)?);
                DelayedStepState::Relation
            }
            DelayedStepState::Relation => {
                cursor.enter("effect.delayed-step.relation");
                cursor.consume("at")?;
                DelayedStepState::Beginning
            }
            DelayedStepState::Beginning => {
                cursor.enter("effect.delayed-step.beginning");
                cursor.consume_phrase(&["the", "beginning", "of", "the", "next"])?;
                DelayedStepState::Turn
            }
            DelayedStepState::Turn => {
                cursor.enter("effect.delayed-step.turn");
                let turn = cursor.consume_type("possessiveTurnWord")?;
                cursor.require(
                    turn.raw.eq_ignore_ascii_case("turn's")
                        || turn.raw.eq_ignore_ascii_case("turn’s"),
                    "the possessive `turn's` reference",
                )?;
                DelayedStepState::Step
            }
            DelayedStepState::Step => {
                cursor.enter("effect.delayed-step.step");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("`upkeep` or `end step`"))?
                    .to_string();
                step = Some(if raw.eq_ignore_ascii_case("upkeep") {
                    cursor.consume("upkeep")?;
                    DelayedTurnStep::Upkeep
                } else if raw.eq_ignore_ascii_case("end") {
                    cursor.consume_phrase(&["end", "step"])?;
                    DelayedTurnStep::EndStep
                } else {
                    return Err(cursor.fail("`upkeep` or `end step`"));
                });
                DelayedStepState::Complete
            }
            DelayedStepState::Complete => {
                cursor.enter("effect.delayed-step.complete");
                return Ok(DelayedStepEffectNode {
                    turn: DelayedTurnReference::NextTurn,
                    step: cursor.require_value(step, "a populated delayed turn step")?,
                    effect: Box::new(
                        cursor.require_value(effect, "a populated delayed child effect")?,
                    ),
                });
            }
        };
    }
}
