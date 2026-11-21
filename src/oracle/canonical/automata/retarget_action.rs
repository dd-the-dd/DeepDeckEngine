//! Stack-target reassignment is one ordinary action, not a copy program.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RetargetActionNode {
    pub player: LocalPlayerReference,
    pub target: TargetListNode,
}

pub(in crate::oracle::canonical) enum RetargetActionAttempt {
    NotClaimed,
    Recognized(RetargetActionNode),
    FailedCommitted(MachineFailure),
}

#[derive(Clone, Copy)]
enum State {
    Actor,
    Choose,
    New,
    Targets,
    For,
    Object,
    Complete,
}

pub(in crate::oracle::canonical) fn try_recognize_retarget_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> RetargetActionAttempt {
    let mut trial = cursor.clone();
    let implicit = trial
        .peek()
        .is_some_and(|s| s.eq_ignore_ascii_case("choose"));
    let head_offset = if implicit {
        0
    } else if trial.peek().is_some_and(|s| s.eq_ignore_ascii_case("you")) {
        1
    } else {
        return RetargetActionAttempt::NotClaimed;
    };
    if !trial
        .peek_ahead(head_offset)
        .is_some_and(|s| s.eq_ignore_ascii_case("choose"))
        || !trial
            .peek_ahead(head_offset + 1)
            .is_some_and(|s| s.eq_ignore_ascii_case("new"))
    {
        return RetargetActionAttempt::NotClaimed;
    }
    let result = (|| -> MachineResult<RetargetActionNode> {
        let mut state = State::Actor;
        let mut player = None;
        let mut target = None;
        loop {
            state = match state {
                State::Actor => {
                    trial.enter("effect.retarget.actor");
                    player = Some(if implicit {
                        LocalPlayerReference::Controller
                    } else {
                        recognize_player_reference(&mut trial, None)?
                    });
                    State::Choose
                }
                State::Choose => {
                    trial.enter("effect.retarget.choose");
                    trial.consume("choose")?;
                    State::New
                }
                State::New => {
                    trial.enter("effect.retarget.new");
                    trial.consume("new")?;
                    State::Targets
                }
                State::Targets => {
                    trial.enter("effect.retarget.targets");
                    trial.consume("targets")?;
                    State::For
                }
                State::For => {
                    trial.enter("effect.retarget.for");
                    trial.consume("for")?;
                    State::Object
                }
                State::Object => {
                    trial.enter("effect.retarget.object");
                    let slots = recognize_target_list(&mut trial)?;
                    trial.enter("effect.retarget.stack-domain");
                    trial.require(slots.slots.len() == 1, "one target stack-object slot")?;
                    let slot = &slots.slots[0];
                    trial.require(
                        slot.cardinality == TargetCardinality::EXACTLY_ONE,
                        "one required stack target",
                    )?;
                    trial.require(
                        stack_domain_selector_supported(&slot.selector),
                        "a spell or stack-ability selector",
                    )?;
                    target = Some(slots);
                    State::Complete
                }
                State::Complete => {
                    trial.enter("effect.retarget.complete");
                    return Ok(RetargetActionNode {
                        player: trial.require_value(player, "a populated choosing actor")?,
                        target: trial.require_value(target, "a populated stack-object target")?,
                    });
                }
            };
        }
    })();
    match result {
        Ok(node) => {
            *cursor = trial;
            RetargetActionAttempt::Recognized(node)
        }
        Err(failure) => RetargetActionAttempt::FailedCommitted(failure),
    }
}

#[cfg(test)]
#[path = "retarget_action_tests.rs"]
mod tests;
