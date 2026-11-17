use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, ActivationTimingAnnotation, EffectRecognitionContext,
    EffectSequenceNode, EffectSequenceParseAttempt, recognize_activation_cost_sequence,
    recognize_activation_timing_annotation, recognize_effect_sequence,
    try_recognize_effect_sequence_before_activation_annotation,
    try_recognize_effect_sequence_with_context,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedEffectSequenceNode {
    pub costs: ActivationCostSequenceNode,
    pub effects: EffectSequenceNode,
    pub timing_annotation: Option<ActivationTimingAnnotation>,
    pub restrictions: super::ActivationRestrictionsNode,
}

pub(in crate::oracle::canonical) enum ActivatedEffectSequenceParseAttempt {
    Parsed(ActivatedEffectSequenceNode),
    UnsupportedSupportLimit(MachineFailure),
    FailedCommitted(MachineFailure),
}

/// Availability for an explicit current-source permanent subject only. This
/// does not establish an event subject or a grammatical pronoun antecedent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ExplicitActivationSourceAvailability {
    #[default]
    Unproved,
    RetainedBattlefieldPermanent,
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
    Ok(ActivatedEffectSequenceNode {
        costs,
        effects,
        timing_annotation: None,
        restrictions: super::ActivationRestrictionsNode::default(),
    })
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
    try_recognize_activation_program(
        cursor,
        name_links,
        name_failure,
        context,
        false,
        |_, context| context.clone(),
    )
}

pub(in crate::oracle::canonical) fn try_recognize_activated_effect_sequence_with_annotation(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> ActivatedEffectSequenceParseAttempt {
    try_recognize_activation_program(
        cursor,
        name_links,
        name_failure,
        context,
        true,
        |_, context| context.clone(),
    )
}

/// The envelope derives body capabilities from parsed costs, exactly once.
/// Existing callers keep their supplied context through the identity adapter.
pub(in crate::oracle::canonical) fn try_recognize_activated_effect_sequence_with_cost_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    derive_context: impl FnOnce(
        &ActivationCostSequenceNode,
        &EffectRecognitionContext,
    ) -> EffectRecognitionContext,
) -> ActivatedEffectSequenceParseAttempt {
    try_recognize_activation_program(
        cursor,
        name_links,
        name_failure,
        context,
        true,
        derive_context,
    )
}

fn try_recognize_activation_program(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    allow_annotation: bool,
    derive_context: impl FnOnce(
        &ActivationCostSequenceNode,
        &EffectRecognitionContext,
    ) -> EffectRecognitionContext,
) -> ActivatedEffectSequenceParseAttempt {
    let checkpoint = cursor.checkpoint();
    let costs = match recognize_activation_prefix(cursor) {
        Ok(costs) => costs,
        Err(failure) => {
            cursor.rollback(checkpoint);
            return ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure);
        }
    };
    let mut context = derive_context(&costs, context);
    // This parent reads `Activate only ...` below and the caller applies it.
    context.ability_restriction_owner = super::AbilityRestrictionOwner::Activated;
    cursor.enter("activation.effect-sequence.effects");
    let effects = match if allow_annotation {
        try_recognize_effect_sequence_before_activation_annotation(
            cursor,
            name_links,
            name_failure,
            &context,
        )
    } else {
        try_recognize_effect_sequence_with_context(cursor, name_links, name_failure, &context)
    } {
        EffectSequenceParseAttempt::Parsed(effects) => effects,
        EffectSequenceParseAttempt::RejectedCandidate(failure) => {
            cursor.rollback(checkpoint);
            return ActivatedEffectSequenceParseAttempt::UnsupportedSupportLimit(failure);
        }
        EffectSequenceParseAttempt::FailedCommitted(failure) => {
            return ActivatedEffectSequenceParseAttempt::FailedCommitted(failure);
        }
    };
    let restrictions = if super::activation_restriction_starts(cursor) {
        match super::recognize_activation_restrictions(cursor) {
            Ok(restrictions) => restrictions,
            Err(failure) => return ActivatedEffectSequenceParseAttempt::FailedCommitted(failure),
        }
    } else {
        super::ActivationRestrictionsNode::default()
    };
    let timing_annotation = if allow_annotation && cursor.peek() == Some("(") {
        match recognize_activation_timing_annotation(cursor) {
            Ok(annotation) => Some(annotation),
            Err(failure) => return ActivatedEffectSequenceParseAttempt::FailedCommitted(failure),
        }
    } else {
        None
    };
    cursor.enter("activation.effect-sequence.complete");
    if let Err(failure) = cursor.finish() {
        return ActivatedEffectSequenceParseAttempt::FailedCommitted(failure);
    }
    ActivatedEffectSequenceParseAttempt::Parsed(ActivatedEffectSequenceNode {
        costs,
        effects,
        timing_annotation,
        restrictions,
    })
}

#[cfg(test)]
#[path = "activated_effect_sequence_tests.rs"]
mod tests;
