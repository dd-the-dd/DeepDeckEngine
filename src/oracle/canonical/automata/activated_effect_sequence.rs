use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, ActivationTimingAnnotation, EffectRecognitionContext,
    EffectSequenceNode, EffectSequenceParseAttempt, recognize_activation_timing_annotation,
    recognize_effect_sequence, try_recognize_effect_sequence_before_activation_annotation,
    try_recognize_effect_sequence_with_context,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedEffectSequenceNode {
    pub costs: ActivationCostSequenceNode,
    pub effects: EffectSequenceNode,
    pub timing_annotation: Option<ActivationTimingAnnotation>,
    pub restrictions: super::ActivationRestrictionsNode,
    /// `This ability costs {N} less to activate for each ...`: activation
    /// metadata after the effects, never an effect (CR 601.2f via 602.2b).
    pub reduction: Option<super::ActivationCostReductionNode>,
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
    /// A battlefield activation whose cost may move the source: a chosen
    /// sacrifice or return that does not exclude it (`Sacrifice a creature:
    /// This creature gets +2/+2`). A self-reference still names the source
    /// (CR 113.7a, 611.2c); if the source left, it is a new object and the
    /// action does nothing to it (CR 400.7).
    RetainedOrDeparted,
}

impl ExplicitActivationSourceAvailability {
    /// True when an explicit self-reference names the battlefield source,
    /// whether or not it is still there when the ability resolves.
    pub(in crate::oracle::canonical) fn names_battlefield_source(self) -> bool {
        matches!(
            self,
            Self::RetainedBattlefieldPermanent | Self::RetainedOrDeparted
        )
    }

    /// Availability proved by an activation's typed costs. `permanent_face`
    /// is the source face's own evidence; a cost that names the source on
    /// the battlefield (`{T}`, `sacrifice this creature`) proves it as well.
    pub(in crate::oracle::canonical) fn from_costs(
        costs: &ActivationCostSequenceNode,
        permanent_face: bool,
    ) -> Self {
        use super::LocalActivationCostNode as Cost;
        let requirements = costs.source_zone_requirements();
        if requirements.hand || requirements.graveyard {
            return Self::Unproved;
        }
        if !(permanent_face || requirements.battlefield) {
            return Self::Unproved;
        }
        let mut possibly_departed = false;
        for cost in &costs.costs {
            match cost {
                Cost::PayMana(_)
                | Cost::TapSelf(_)
                | Cost::PayLife(_)
                | Cost::PayPlayerCounters(_)
                | Cost::TapObjects(_)
                | Cost::DiscardCard(_)
                | Cost::RemoveSourceCounters(_)
                | Cost::RemoveControlledCounters(_)
                | Cost::ExileCards(_)
                | Cost::PutSourceCounters(_)
                | Cost::PutChosenCounters(_) => {}
                // `Sacrifice another ...` cannot move the source (CR 109.1);
                // without `another` the payer may choose the source itself.
                Cost::SacrificeObject(selected) => possibly_departed |= !selected.exclude_source,
                Cost::ReturnObjects(selected) => possibly_departed |= !selected.exclude_source,
                // The source surely leaves: a source subject reads its last
                // known information through its own event-subject context.
                Cost::ReturnSelf
                | Cost::SacrificeSelf(_)
                | Cost::DiscardSelf(_)
                | Cost::ExileSelf(_) => return Self::Unproved,
            }
        }
        if possibly_departed {
            Self::RetainedOrDeparted
        } else {
            Self::RetainedBattlefieldPermanent
        }
    }
}

fn recognize_activation_prefix(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
) -> MachineResult<ActivationCostSequenceNode> {
    // CR 207.2c: an ability word (`Channel —`) labels the ability and has
    // no rules meaning; the costs start after its em dash.
    let label_width = cursor
        .token_slice(cursor.position(), usize::MAX)
        .iter()
        .take_while(|token| token.token_type == "abilityWord")
        .count();
    if label_width > 0
        && cursor
            .peek_ahead(label_width)
            .is_some_and(|raw| raw == "\u{2014}")
    {
        cursor.enter("activation.effect-sequence.label");
        cursor.advance_to(cursor.position() + label_width + 1)?;
    }
    cursor.enter("activation.effect-sequence.costs");
    let costs = super::recognize_named_activation_cost_sequence(cursor, name_links)?;
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
    let costs = recognize_activation_prefix(cursor, name_links)?;
    cursor.enter("activation.effect-sequence.effects");
    let effects = recognize_effect_sequence(cursor, name_links, name_failure)?;
    cursor.enter("activation.effect-sequence.complete");
    cursor.finish()?;
    Ok(ActivatedEffectSequenceNode {
        costs,
        effects,
        timing_annotation: None,
        restrictions: super::ActivationRestrictionsNode::default(),
        reduction: None,
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
    let costs = match recognize_activation_prefix(cursor, name_links) {
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
    let reduction = if allow_annotation
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("ability"))
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("costs"))
    {
        match super::recognize_activation_cost_reduction(cursor) {
            Ok(reduction) => Some(reduction),
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
        reduction,
    })
}

#[cfg(test)]
#[path = "activated_effect_sequence_tests.rs"]
mod tests;
