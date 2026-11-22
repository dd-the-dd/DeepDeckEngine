use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DelayedTurnStep, EffectRecognitionContext, LocalEffectNode, LocalObjectReference,
    LocalTriggerEventNode, PutCountersOnObjectEffectNode, TargetBinding, ZoneChangeAction,
    ZoneChangeEffectNode, recognize_put_counters_effect, recognize_source_attacked_event,
    recognize_zone_change_effect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BoundDelayedReturnNode {
    pub object: TargetBinding,
    pub step: DelayedTurnStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReturnedUnderControllerConditionNode {
    pub object: TargetBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AttackDelayedReturnCounterNode {
    pub event: LocalTriggerEventNode,
    pub exile: ZoneChangeEffectNode,
    pub delayed_return: BoundDelayedReturnNode,
    pub condition: ReturnedUnderControllerConditionNode,
    pub counters: PutCountersOnObjectEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SequenceState {
    Trigger,
    Event,
    EventSeparator,
    Exile,
    ExileTerminator,
    DelayedTiming,
    Return,
    ReturnTerminator,
    Condition,
    ConditionSeparator,
    Counters,
    CountersTerminator,
    Complete,
}

#[derive(Default)]
struct SequenceFrame {
    event: Option<LocalTriggerEventNode>,
    exile: Option<ZoneChangeEffectNode>,
    delayed_return: Option<BoundDelayedReturnNode>,
    condition: Option<ReturnedUnderControllerConditionNode>,
    counters: Option<PutCountersOnObjectEffectNode>,
}

/// Recognizes the leading delayed envelope `At the beginning of the next
/// STEP,`. The scheduled child remains owned by the calling sequence.
pub(in crate::oracle::canonical) fn recognize_next_step_prefix(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DelayedTurnStep> {
    cursor.enter("duration.next-step.relation");
    cursor.consume("at")?;
    cursor.enter("duration.next-step.beginning");
    cursor.consume_phrase(&["the", "beginning", "of", "the", "next"])?;
    cursor.enter("duration.next-step.step");
    let step = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("upkeep"))
    {
        cursor.consume("upkeep")?;
        DelayedTurnStep::Upkeep
    } else {
        cursor.consume_phrase(&["end", "step"])?;
        DelayedTurnStep::EndStep
    };
    cursor.enter("duration.next-step.separator");
    cursor.consume(",")?;
    Ok(step)
}

/// Recognizes `return that card to the battlefield under its owner's
/// control`, resolving the anaphor through the binding supplied by the parent.
fn recognize_bound_return_to_owner_control(
    cursor: &mut OracleTokenCursor<'_>,
    object: TargetBinding,
    step: DelayedTurnStep,
) -> MachineResult<BoundDelayedReturnNode> {
    cursor.enter("effect.bound-return.action");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Return,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.enter("effect.bound-return.object");
    cursor.consume_phrase(&["that", "card"])?;
    cursor.enter("effect.bound-return.destination");
    cursor.consume_phrase(&["to", "the", "battlefield", "under", "its"])?;
    cursor.enter("effect.bound-return.owner");
    let owner = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("owner's") || raw.eq_ignore_ascii_case("owner’s"))
        .ok_or_else(|| cursor.fail("`owner's`"))?
        .to_string();
    cursor.consume(&owner)?;
    cursor.enter("effect.bound-return.control");
    cursor.consume("control")?;
    Ok(BoundDelayedReturnNode { object, step })
}

/// Recognizes `if it entered under your control,` against the returned-object
/// binding chosen by the parent sequence.
fn recognize_returned_under_controller_condition(
    cursor: &mut OracleTokenCursor<'_>,
    object: TargetBinding,
) -> MachineResult<ReturnedUnderControllerConditionNode> {
    cursor.enter("condition.returned-under-controller.if");
    cursor.consume("if")?;
    cursor.enter("condition.returned-under-controller.object");
    cursor.consume("it")?;
    cursor.enter("condition.returned-under-controller.event");
    cursor.consume_phrase(&["entered", "under", "your", "control"])?;
    Ok(ReturnedUnderControllerConditionNode { object })
}

/// Composes an attack trigger, an atomic targeted exile, one delayed bound
/// return, and one conditional source-counter effect. Every child retains its
/// own typed result; this parent owns only ordering, punctuation, and bindings.
pub(in crate::oracle::canonical) fn recognize_attack_delayed_return_counter(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<AttackDelayedReturnCounterNode> {
    let mut state = SequenceState::Trigger;
    let mut frame = SequenceFrame::default();
    let target_binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    loop {
        state = match state {
            SequenceState::Trigger => {
                cursor.enter("trigger.attack-delayed-return.trigger");
                cursor.consume("whenever")?;
                SequenceState::Event
            }
            SequenceState::Event => {
                cursor.enter("trigger.attack-delayed-return.event");
                frame.event = Some(recognize_source_attacked_event(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                SequenceState::EventSeparator
            }
            SequenceState::EventSeparator => {
                cursor.enter("trigger.attack-delayed-return.event-separator");
                cursor.consume(",")?;
                SequenceState::Exile
            }
            SequenceState::Exile => {
                cursor.enter("trigger.attack-delayed-return.exile");
                frame.exile = Some(match recognize_zone_change_effect(cursor)? {
                    LocalEffectNode::ZoneChange(effect)
                        if effect.action == ZoneChangeAction::Exile =>
                    {
                        effect
                    }
                    _ => return Err(cursor.fail("an atomic targeted exile effect")),
                });
                SequenceState::ExileTerminator
            }
            SequenceState::ExileTerminator => {
                cursor.enter("trigger.attack-delayed-return.exile-terminator");
                cursor.consume(".")?;
                SequenceState::DelayedTiming
            }
            SequenceState::DelayedTiming => {
                cursor.enter("trigger.attack-delayed-return.delayed-timing");
                let step = recognize_next_step_prefix(cursor)?;
                frame.delayed_return = Some(recognize_bound_return_to_owner_control(
                    cursor,
                    target_binding,
                    step,
                )?);
                SequenceState::Return
            }
            SequenceState::Return => SequenceState::ReturnTerminator,
            SequenceState::ReturnTerminator => {
                cursor.enter("trigger.attack-delayed-return.return-terminator");
                cursor.consume(".")?;
                SequenceState::Condition
            }
            SequenceState::Condition => {
                cursor.enter("trigger.attack-delayed-return.condition");
                frame.condition = Some(recognize_returned_under_controller_condition(
                    cursor,
                    target_binding,
                )?);
                SequenceState::ConditionSeparator
            }
            SequenceState::ConditionSeparator => {
                cursor.enter("trigger.attack-delayed-return.condition-separator");
                cursor.consume(",")?;
                SequenceState::Counters
            }
            SequenceState::Counters => {
                cursor.enter("trigger.attack-delayed-return.counters");
                frame.counters = Some(recognize_put_counters_effect(
                    cursor,
                    name_links,
                    name_failure,
                    &EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf),
                )?);
                SequenceState::CountersTerminator
            }
            SequenceState::CountersTerminator => {
                cursor.enter("trigger.attack-delayed-return.counters-terminator");
                cursor.consume(".")?;
                SequenceState::Complete
            }
            SequenceState::Complete => {
                cursor.enter("trigger.attack-delayed-return.complete");
                cursor.finish()?;
                return Ok(AttackDelayedReturnCounterNode {
                    event: cursor.require_value(frame.event, "the attack event")?,
                    exile: cursor.require_value(frame.exile, "the targeted exile")?,
                    delayed_return: cursor
                        .require_value(frame.delayed_return, "the delayed bound return")?,
                    condition: cursor.require_value(
                        frame.condition,
                        "the returned-object controller condition",
                    )?,
                    counters: cursor.require_value(frame.counters, "the counter effect")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "delayed_blink_tests.rs"]
mod tests;
