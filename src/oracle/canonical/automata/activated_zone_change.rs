use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, TargetCardZoneChangeEffectNode, recognize_activation_cost_sequence,
    recognize_target_card_zone_change_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedCardZoneChangeNode {
    pub costs: ActivationCostSequenceNode,
    pub effect: TargetCardZoneChangeEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivatedCardZoneChangeState {
    Costs,
    EffectSeparator,
    Effect,
    Terminator,
    Complete,
}

#[derive(Default)]
struct ActivatedCardZoneChangeFrame {
    costs: Option<ActivationCostSequenceNode>,
    effect: Option<TargetCardZoneChangeEffectNode>,
}

/// Recognizes an activated, targeted card move whose source is a zone rather
/// than the battlefield. Cost recognition is delegated to the shared
/// activation-cost automaton; this machine owns only the effect envelope and
/// its card-zone relation.
pub(in crate::oracle::canonical) fn recognize_activated_card_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivatedCardZoneChangeNode> {
    recognize_activated_card_zone_change_internal(cursor).map_err(|failure| match failure {
        CardZoneActivationFailure::Malformed(failure)
        | CardZoneActivationFailure::ProgramEnvelope(failure) => failure,
    })
}

pub(in crate::oracle::canonical) enum ActivatedCardZoneChangeParseAttempt {
    Parsed(ActivatedCardZoneChangeNode),
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
}

enum CardZoneActivationFailure {
    Malformed(MachineFailure),
    ProgramEnvelope(MachineFailure),
}
impl From<MachineFailure> for CardZoneActivationFailure {
    fn from(failure: MachineFailure) -> Self {
        Self::Malformed(failure)
    }
}

pub(in crate::oracle::canonical) fn try_recognize_activated_card_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
) -> ActivatedCardZoneChangeParseAttempt {
    let checkpoint = cursor.checkpoint();
    match recognize_activated_card_zone_change_internal(cursor) {
        Ok(node) => ActivatedCardZoneChangeParseAttempt::Parsed(node),
        Err(CardZoneActivationFailure::ProgramEnvelope(failure)) => {
            cursor.rollback(checkpoint);
            ActivatedCardZoneChangeParseAttempt::RejectedCandidate(failure)
        }
        Err(CardZoneActivationFailure::Malformed(failure)) => {
            ActivatedCardZoneChangeParseAttempt::FailedCommitted(failure)
        }
    }
}

fn recognize_activated_card_zone_change_internal(
    cursor: &mut OracleTokenCursor<'_>,
) -> Result<ActivatedCardZoneChangeNode, CardZoneActivationFailure> {
    let mut state = ActivatedCardZoneChangeState::Costs;
    let mut frame = ActivatedCardZoneChangeFrame::default();
    loop {
        state = match state {
            ActivatedCardZoneChangeState::Costs => {
                cursor.enter("activation.card-zone-change.costs");
                frame.costs = Some(recognize_activation_cost_sequence(cursor)?);
                ActivatedCardZoneChangeState::EffectSeparator
            }
            ActivatedCardZoneChangeState::EffectSeparator => {
                cursor.enter("activation.card-zone-change.effect-separator");
                cursor.consume(":")?;
                ActivatedCardZoneChangeState::Effect
            }
            ActivatedCardZoneChangeState::Effect => {
                cursor.enter("effect.activated-card-zone-change.effect");
                let action = match super::try_recognize_target_card_action(cursor) {
                    super::CollectionOperandAttempt::Recognized(fragment) => fragment.node,
                    super::CollectionOperandAttempt::FailedCommitted(failure)
                    | super::CollectionOperandAttempt::RejectedCandidate(failure) => {
                        return Err(CardZoneActivationFailure::Malformed(failure));
                    }
                    super::CollectionOperandAttempt::NotClaimed => {
                        return Err(CardZoneActivationFailure::ProgramEnvelope(
                            cursor.fail("a targeted card movement"),
                        ));
                    }
                };
                let Some(effect) = super::legacy_target_card_move(&action) else {
                    return Err(CardZoneActivationFailure::ProgramEnvelope(
                        cursor.fail("a movement delegated to the shared effect program"),
                    ));
                };
                frame.effect = Some(effect);
                ActivatedCardZoneChangeState::Terminator
            }
            ActivatedCardZoneChangeState::Terminator => {
                cursor.enter("effect.activated-card-zone-change.terminator");
                if cursor.peek().is_some_and(|raw| raw != ".") {
                    return Err(CardZoneActivationFailure::ProgramEnvelope(
                        cursor.fail("one atomic targeted card-zone action"),
                    ));
                }
                cursor.consume(".")?;
                ActivatedCardZoneChangeState::Complete
            }
            ActivatedCardZoneChangeState::Complete => {
                cursor.enter("effect.activated-card-zone-change.complete");
                cursor
                    .finish()
                    .map_err(CardZoneActivationFailure::ProgramEnvelope)?;
                return Ok(ActivatedCardZoneChangeNode {
                    costs: cursor.require_value(frame.costs, "activation costs")?,
                    effect: cursor.require_value(frame.effect, "a targeted card-zone effect")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "activated_zone_change_tests.rs"]
mod tests;
