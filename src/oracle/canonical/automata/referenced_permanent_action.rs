//! Zone actions on one bound permanent reference (`destroy|exile|sacrifice
//! it|that NOUN|the NOUN`): one reader, the verb's action as data.
use super::super::{MachineResult, OracleTokenCursor};

/// `destroy REFERENCE`: the destruction of one bound permanent reference
/// (the shared contextual operand), with an optional regeneration
/// prohibition (CR 701.19c). Target declarations stay with the targeted
/// zone change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReferencedPermanentActionNode {
    pub object: super::PermanentEffectReference,
    pub cannot_regenerate: bool,
    /// The zone action on the bound permanent.
    pub action: ReferencedPermanentAction,
}

/// What happens to one bound permanent reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReferencedPermanentAction {
    /// CR 701.8.
    Destroy,
    /// CR 701.13.
    Exile,
    /// CR 701.21: only its controller sacrifices it.
    Sacrifice,
}

/// `exile|sacrifice REFERENCE` on a permanent the program bound other than
/// the ability's source (`exile it` after a card was returned to the
/// battlefield, `sacrifice the creature`): the shared reader below, claimed
/// only when it reads such a reference. Source references keep their own
/// readers; anything else is not claimed.
pub(in crate::oracle::canonical) fn recognize_referenced_permanent_zone_action(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> ReferencedPermanentActionAttempt {
    let action = if super::lemma_at(cursor, super::VerbLemma::Exile, super::VerbAgreement::Base) {
        ReferencedPermanentAction::Exile
    } else if super::lemma_at(
        cursor,
        super::VerbLemma::Sacrifice,
        super::VerbAgreement::Base,
    ) {
        ReferencedPermanentAction::Sacrifice
    } else {
        return ReferencedPermanentActionAttempt::NotClaimed;
    };
    // The claim is the bound reference after the verb: before it nothing is
    // committed; once a non-source reference is read, its failures are the
    // action's (the regeneration clause, the trailing frame).
    let mut probe = cursor.clone();
    probe.enter("effect.referenced-permanent.claim");
    if probe.advance_to(probe.position() + 1).is_err() {
        return ReferencedPermanentActionAttempt::NotClaimed;
    }
    let reference = if super::definite_object_anaphor_starts(&probe) {
        super::recognize_definite_object_reference(
            &mut probe,
            &super::BindingEnvironment::for_permanent_context(context),
        )
    } else {
        super::recognize_contextual_permanent_operand(&mut probe, name_links, name_failure, context)
            .and_then(|operand| match operand {
                super::PermanentOperandNode::BoundReference(object) => Ok(object),
                _ => Err(probe.fail("a bound permanent reference")),
            })
    };
    match reference {
        Ok(super::PermanentEffectReference::Source(_)) | Err(_) => {
            ReferencedPermanentActionAttempt::NotClaimed
        }
        Ok(_) => {
            let mut trial = cursor.clone();
            match recognize_referenced_permanent_action(
                &mut trial,
                action,
                name_links,
                name_failure,
                context,
            ) {
                Ok(node) => {
                    *cursor = trial;
                    ReferencedPermanentActionAttempt::Parsed(node)
                }
                Err(failure) => ReferencedPermanentActionAttempt::FailedCommitted(failure),
            }
        }
    }
}

/// The transactional outcome of the zone action on a bound permanent.
pub(in crate::oracle::canonical) enum ReferencedPermanentActionAttempt {
    /// No verb of the family, or no non-source bound reference after it.
    NotClaimed,
    /// The reference was claimed; the failure is the action's.
    FailedCommitted(super::super::MachineFailure),
    Parsed(ReferencedPermanentActionNode),
}

/// `destroy REFERENCE` (the established destroy reader).
pub(in crate::oracle::canonical) fn recognize_destroy_reference_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> MachineResult<ReferencedPermanentActionNode> {
    recognize_referenced_permanent_action(
        cursor,
        ReferencedPermanentAction::Destroy,
        name_links,
        name_failure,
        context,
    )
}

/// One reader for the zone actions on one bound permanent reference
/// (`destroy|exile|sacrifice it|that NOUN|the NOUN`): the verb, the shared
/// reference operand (a definite description through the shared resolver),
/// and destruction's regeneration prohibition (CR 701.19c).
pub(in crate::oracle::canonical) fn recognize_referenced_permanent_action(
    cursor: &mut OracleTokenCursor<'_>,
    action: ReferencedPermanentAction,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
    context: &super::EffectRecognitionContext,
) -> MachineResult<ReferencedPermanentActionNode> {
    let (verb_state, object_state) = match action {
        ReferencedPermanentAction::Destroy => (
            "effect.destroy-reference.action",
            "effect.destroy-reference.object",
        ),
        ReferencedPermanentAction::Exile | ReferencedPermanentAction::Sacrifice => (
            "effect.referenced-permanent.action",
            "effect.referenced-permanent.object",
        ),
    };
    cursor.enter(verb_state);
    match action {
        ReferencedPermanentAction::Destroy => cursor.consume("destroy")?,
        ReferencedPermanentAction::Exile => {
            super::consume_lemma(cursor, super::VerbLemma::Exile, super::VerbAgreement::Base)?
        }
        ReferencedPermanentAction::Sacrifice => super::consume_lemma(
            cursor,
            super::VerbLemma::Sacrifice,
            super::VerbAgreement::Base,
        )?,
    }
    cursor.enter(object_state);
    let object = if super::definite_object_anaphor_starts(cursor) {
        super::recognize_definite_object_reference(
            cursor,
            &super::BindingEnvironment::for_permanent_context(context),
        )?
    } else {
        let operand = super::recognize_contextual_permanent_operand(
            cursor,
            name_links,
            name_failure,
            context,
        )?;
        let super::PermanentOperandNode::BoundReference(object) = operand else {
            return Err(cursor.fail("a bound permanent reference"));
        };
        object
    };
    let cannot_regenerate = match action {
        ReferencedPermanentAction::Destroy => {
            super::recognize_scalar_regeneration_amendment(cursor)?
        }
        ReferencedPermanentAction::Exile | ReferencedPermanentAction::Sacrifice => false,
    };
    Ok(ReferencedPermanentActionNode {
        object,
        cannot_regenerate,
        action,
    })
}
