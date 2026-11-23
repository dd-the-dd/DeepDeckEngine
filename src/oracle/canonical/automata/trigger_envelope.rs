//! One typed event envelope for the shared triggered-program parent: an
//! optional ability word and the one event clause (`event_clause.rs`). The
//! following guard, permission and effect tokens remain parent-owned.

use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EventNumericBinding, LocalObjectReference, LocalTriggerEventNode, PermanentDiedEventNode,
    SacrificeEventNode, SourceTriggerEnvelopeNode, SourceTriggerEventKind, SpellCastEventNode,
    StepEventNode, TypedObservedEventObject,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentTransitionKind {
    Entered,
    Died,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EntryStatePredicate {
    Tapped,
    Untapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum GenericTriggerEventNode {
    SourceTransition(SourceTriggerEnvelopeNode),
    SpellCast(SpellCastEventNode),
    Sacrificed(SacrificeEventNode),
    ActorObjectAlternatives(super::ActorObjectEventAlternativesNode),
    Step(StepEventNode),
    PermanentTransition {
        kind: PermanentTransitionKind,
        subjects: PermanentDiedEventNode,
        entry_state: Option<EntryStatePredicate>,
        /// `enters or dies`: the other transition of the same subjects, an
        /// alternative event (one occurrence each, never both at once).
        union_kind: Option<PermanentTransitionKind>,
    },
    Local(LocalTriggerEventNode),
    /// Shared quantified subject + event verb (`one or more cards leave …`).
    Object(super::ObjectEventNode),
    /// The ability source with one or more event verbs (`attacks or blocks`).
    SourceEvent(super::SourceEventUnionNode),
    /// Counters put on a permanent (CR 122.6).
    CounterPlacement(super::CounterPlacementEventNode),
    /// A targeted-object union `becomes the target of` a spell or ability
    /// (CR 115.1). The source subject is a `SourceEvent` verb.
    BecameTarget(super::BecameTargetEventNode),
}

impl GenericTriggerEventNode {
    /// Recognition alternatives are not event cardinality. Every matched
    /// permanent-entry occurrence observes one entering object, even when
    /// several OR branches match it. Dying/other-event envelopes do not supply
    /// this battlefield-entry reference.
    pub(in crate::oracle::canonical) fn observed_object(&self) -> Option<TypedObservedEventObject> {
        let entering = match self {
            Self::SourceTransition(event) => {
                event.event == SourceTriggerEventKind::Entered && !event.is_union()
            }
            Self::PermanentTransition {
                kind: PermanentTransitionKind::Entered,
                subjects,
                union_kind: None,
                ..
            } => !subjects.alternatives.is_empty(),
            Self::Local(LocalTriggerEventNode::SourceEntered { .. }) => true,
            _ => false,
        };
        entering.then_some(TypedObservedEventObject::EnteringPermanent)
    }

    pub(in crate::oracle::canonical) fn observed_object_is_source(&self) -> bool {
        matches!(self, Self::SourceTransition(event) if event.event == SourceTriggerEventKind::Entered && !event.is_union())
            || matches!(
                self,
                Self::Local(LocalTriggerEventNode::SourceEntered { .. })
            )
    }

    pub(in crate::oracle::canonical) fn numeric_binding(&self) -> Option<EventNumericBinding> {
        match self {
            Self::Local(
                LocalTriggerEventNode::CombatDamageToPlayer { amount, .. }
                | LocalTriggerEventNode::SourceDealtDamage { amount, .. },
            ) => Some(*amount),
            // The runtime exports the dealt amount of every dealt-damage observer.
            Self::Local(LocalTriggerEventNode::DamageDealt { .. })
            | Self::Object(super::ObjectEventNode {
                verb: super::ObjectEventVerb::DealDamage,
                ..
            }) => Some(EventNumericBinding::DamageAmount),
            // The runtime exports the dealt amount for every received-damage
            // observer: per object, per player, and the one-or-more total.
            Self::Local(LocalTriggerEventNode::PlayerEvent { actions, .. })
                if matches!(
                    actions.as_slice(),
                    [super::PlayerEventAction::BeDealtDamage(_)]
                ) =>
            {
                Some(EventNumericBinding::DamageAmount)
            }
            Self::Object(super::ObjectEventNode {
                verb: super::ObjectEventVerb::BeDealtDamage(_),
                ..
            }) => Some(EventNumericBinding::DamageAmount),
            _ => None,
        }
    }

    /// A cycling trigger functions while its card is cycled from the hand
    /// (CR 702.29), so any card type may own it. Every other event here is
    /// observed by a permanent on the battlefield.
    pub(in crate::oracle::canonical) fn requires_permanent_face(&self) -> bool {
        !matches!(
            self,
            Self::Local(LocalTriggerEventNode::SourceCycled { .. })
        )
    }

    pub(in crate::oracle::canonical) fn source(&self) -> LocalObjectReference {
        // The enclosing card owns this ability, including events observing
        // somebody else. Do not substitute the triggering object for self.
        LocalObjectReference::SourceSelf
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct GenericTriggerEnvelopeNode {
    pub event: GenericTriggerEventNode,
    pub body_start: usize,
    pub has_ability_word: bool,
}

pub(in crate::oracle::canonical) enum GenericTriggerEnvelopeAttempt {
    NotClaimed,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Recognized(GenericTriggerEnvelopeNode),
}

fn recognize_optional_ability_word(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    let labelled = cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "abilityWord");
    if !labelled {
        return Ok(false);
    }
    cursor.enter("trigger.envelope.ability-word");
    while cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "abilityWord")
    {
        cursor.consume_type("abilityWord")?;
    }
    cursor.enter("trigger.envelope.ability-word-separator");
    cursor.consume("—")?;
    Ok(true)
}

fn recognized(
    cursor: &OracleTokenCursor<'_>,
    event: GenericTriggerEventNode,
    has_ability_word: bool,
) -> GenericTriggerEnvelopeAttempt {
    GenericTriggerEnvelopeAttempt::Recognized(GenericTriggerEnvelopeNode {
        event,
        body_start: cursor.position(),
        has_ability_word,
    })
}

/// `[ABILITY WORD —] EVENT-CLAUSE ,`: the ability word and label are the
/// envelope's; the trigger word, occurrence and comma are read by the one
/// event clause reader (`event_clause.rs`), which returns immediately after
/// the comma. In particular this reader never consumes `if` or `you may`
/// from the body.
///
/// The structural introducer (`at`, `when`, `whenever`, CR 603.1) claims the
/// ability for this frame: when no event alternative is complete, the
/// alternative that read furthest becomes the committed diagnostic.
pub(in crate::oracle::canonical) fn try_recognize_generic_trigger_envelope(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> GenericTriggerEnvelopeAttempt {
    let original = cursor.checkpoint();
    let has_ability_word = match recognize_optional_ability_word(cursor) {
        Ok(value) => value,
        Err(failure) => return GenericTriggerEnvelopeAttempt::RejectedCandidate(failure),
    };
    let claimed = super::event_introducer_at(cursor).is_some();
    if !claimed && let Some(failure) = unclassified_label_before_trigger(cursor) {
        return GenericTriggerEnvelopeAttempt::FailedCommitted(failure);
    }
    match super::read_event_clause(cursor, name_links, name_failure, ",") {
        super::EventClauseAttempt::Recognized(event) => recognized(cursor, event, has_ability_word),
        super::EventClauseAttempt::Committed(failure) => {
            GenericTriggerEnvelopeAttempt::FailedCommitted(failure)
        }
        super::EventClauseAttempt::Rejected(failure) if claimed => {
            GenericTriggerEnvelopeAttempt::FailedCommitted(failure)
        }
        super::EventClauseAttempt::Rejected(failure) => {
            GenericTriggerEnvelopeAttempt::RejectedCandidate(failure)
        }
        super::EventClauseAttempt::NotClaimed => {
            cursor.rollback(original);
            GenericTriggerEnvelopeAttempt::NotClaimed
        }
    }
}

/// A short label closed by an em dash and followed by a trigger word is the
/// structural shape of a labelled trigger (CR 207.2c, 603.1). When the label
/// is neither an ability word nor a rules-free flavor word (for example it
/// names a mechanic or a cost), the trigger frame still owns the ability and
/// reports the unread label instead of leaving the ability unclaimed.
fn unclassified_label_before_trigger(cursor: &mut OracleTokenCursor<'_>) -> Option<MachineFailure> {
    const MAXIMUM_LABEL_TOKENS: usize = 8;
    let dash =
        (1..=MAXIMUM_LABEL_TOKENS).find(|offset| cursor.peek_ahead(*offset) == Some("\u{2014}"))?;
    cursor.peek_ahead(dash + 1).filter(|raw| {
        ["at", "when", "whenever"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
    })?;
    cursor.enter("trigger.envelope.label");
    Some(cursor.fail("an ability word or a rules-free flavor word before the trigger word"))
}

#[cfg(test)]
#[path = "trigger_envelope_tests.rs"]
mod tests;
