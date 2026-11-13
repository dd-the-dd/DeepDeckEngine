use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CounterEffectNode, DelayedStepEffectNode, LocalEffectNode, TargetBinding,
    recognize_counter_effect, recognize_delayed_draw,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterDelayedDrawNode {
    pub counter: CounterEffectNode,
    pub delayed: DelayedStepEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterDelayedDrawState {
    Counter,
    CounterBoundary,
    DelayedDraw,
    Terminator,
    Complete,
}

/// Composes an atomic counter effect with a delayed draw whose explicit player
/// subject may refer to the counter target. The parent owns only sentence order
/// and the antecedent binding; target, reference, numeric, draw, and timing
/// semantics remain in their reusable child machines.
pub(in crate::oracle::canonical) fn recognize_counter_delayed_draw(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterDelayedDrawNode> {
    let mut state = CounterDelayedDrawState::Counter;
    let mut counter = None;
    let mut delayed = None;
    loop {
        state = match state {
            CounterDelayedDrawState::Counter => {
                cursor.enter("sequence.counter-delayed-draw.counter");
                counter = Some(match recognize_counter_effect(cursor)? {
                    LocalEffectNode::CounterSpell(counter) => counter,
                    _ => return Err(cursor.fail("an atomic counter effect")),
                });
                CounterDelayedDrawState::CounterBoundary
            }
            CounterDelayedDrawState::CounterBoundary => {
                cursor.enter("sequence.counter-delayed-draw.counter-boundary");
                cursor.consume(".")?;
                CounterDelayedDrawState::DelayedDraw
            }
            CounterDelayedDrawState::DelayedDraw => {
                cursor.enter("sequence.counter-delayed-draw.delayed-draw");
                let antecedent = TargetBinding {
                    effect_index: 0,
                    slot_index: 0,
                };
                delayed = Some(recognize_delayed_draw(cursor, Some(antecedent))?.delayed);
                CounterDelayedDrawState::Terminator
            }
            CounterDelayedDrawState::Terminator => {
                cursor.enter("sequence.counter-delayed-draw.terminator");
                cursor.consume(".")?;
                CounterDelayedDrawState::Complete
            }
            CounterDelayedDrawState::Complete => {
                cursor.enter("sequence.counter-delayed-draw.complete");
                cursor.finish()?;
                return Ok(CounterDelayedDrawNode {
                    counter: cursor.require_value(counter, "a populated counter effect")?,
                    delayed: cursor.require_value(delayed, "a populated delayed draw")?,
                });
            }
        };
    }
}
