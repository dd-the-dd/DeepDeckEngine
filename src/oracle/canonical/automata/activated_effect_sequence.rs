use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, EffectRecognitionContext, EffectSequenceNode,
    EffectSequenceParseAttempt, recognize_activation_cost_sequence, recognize_effect_sequence,
    try_recognize_effect_sequence_with_context,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedEffectSequenceNode {
    pub costs: ActivationCostSequenceNode,
    pub effects: EffectSequenceNode,
}

pub(in crate::oracle::canonical) enum ActivatedEffectSequenceParseAttempt {
    Parsed(ActivatedEffectSequenceNode),
    UnsupportedSupportLimit(MachineFailure),
    FailedCommitted(MachineFailure),
}

fn recognize_activation_prefix(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationCostSequenceNode> {
    cursor.enter("activation.effect-sequence.costs");
    let costs = recognize_activation_cost_sequence(cursor)?;
    cursor.enter("activation.effect-sequence.separator");
    cursor.consume(":")?;
    Ok(costs)
}

/// Composes the shared activation-cost and effect-sequence machines around
/// the colon owned by an activated-ability envelope.
pub(in crate::oracle::canonical) fn recognize_activated_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ActivatedEffectSequenceNode> {
    let costs = recognize_activation_prefix(cursor)?;
    cursor.enter("activation.effect-sequence.effects");
    let effects = recognize_effect_sequence(cursor, name_links, name_failure)?;
    cursor.enter("activation.effect-sequence.complete");
    cursor.finish()?;
    Ok(ActivatedEffectSequenceNode { costs, effects })
}

/// Parse-once migration boundary for activated programs. The parent candidate
/// scanner does not validate costs or effects. Unsupported children restore
/// the entire activation checkpoint; an explicit effect commitment is retained
/// as a typed failure rather than inferred from cursor position or trace text.
pub(in crate::oracle::canonical) fn try_recognize_activated_effect_sequence_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> ActivatedEffectSequenceParseAttempt {
    let checkpoint = cursor.checkpoint();
    let costs = match recognize_activation_prefix(cursor) {
        Ok(costs) => costs,
        Err(failure) => {
            cursor.rollback(checkpoint);
            return ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure);
        }
    };
    cursor.enter("activation.effect-sequence.effects");
    let effects =
        match try_recognize_effect_sequence_with_context(cursor, name_links, name_failure, context)
        {
            EffectSequenceParseAttempt::Parsed(effects) => effects,
            EffectSequenceParseAttempt::RejectedCandidate(failure) => {
                cursor.rollback(checkpoint);
                return ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure);
            }
            EffectSequenceParseAttempt::FailedCommitted(failure) => {
                return ActivatedEffectSequenceParseAttempt::FailedCommitted(failure);
            }
        };
    cursor.enter("activation.effect-sequence.complete");
    if let Err(failure) = cursor.finish() {
        return ActivatedEffectSequenceParseAttempt::FailedCommitted(failure);
    }
    ActivatedEffectSequenceParseAttempt::Parsed(ActivatedEffectSequenceNode { costs, effects })
}

#[cfg(test)]
#[path = "activated_effect_sequence_tests.rs"]
mod tests;
