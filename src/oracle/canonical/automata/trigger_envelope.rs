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
        /// `enters from a graveyard`: the zone the entering object left.
        origin: Option<super::EntryOriginNode>,
        /// `enters or dies`: the other transition of the same subjects, an
        /// alternative event (one occurrence each, never both at once).
        union_kind: Option<PermanentTransitionKind>,
        /// `during combat` (CR 506.1): the transition happens in a combat step.
        during_combat: bool,
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
    /// A trigger condition list (CR 603.2): `EVENT and when|whenever|at the
    /// beginning of EVENT ...`. Each member is a complete typed clause with
    /// its own introducer; the ability triggers once per occurrence of any
    /// member. Members are never nested lists.
    Compound(Vec<GenericTriggerEventNode>),
    /// An occurrence with its event-time condition (`while CONDITION`, a turn
    /// owner without pattern matcher). The trigger envelope hoists the
    /// condition of a single occurrence to the ability; a compound member
    /// keeps it as an attribute of that member's pattern.
    Conditioned {
        event: Box<GenericTriggerEventNode>,
        condition: super::EventTimeConditionNode,
    },
    /// `SOURCE has N or more|no COUNTERS on it` (CR 603.8): a state trigger,
    /// checked whenever a player would receive priority.
    SourceCounterState(SourceCounterStateNode),
    /// `When CONDITION,` (CR 603.8): a state trigger over the shared
    /// condition language (`you control no Islands`, `you have 30 or more
    /// life`), read by `state_trigger.rs`.
    StateCondition(super::SentenceConditionNode),
}

/// The counter threshold a state-triggered ability watches on its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceCounterStateNode {
    pub source: LocalObjectReference,
    pub counter: crate::oracle::counter_name::CounterName,
    pub comparison: CounterStateComparison,
}

/// `has no COUNTERS` is a count of zero; `has N or more COUNTERS` is at least
/// N; `N or fewer` / `at most N` and `exactly N` bound it from above or both
/// ways. Read through the shared count threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterStateComparison {
    None,
    AtLeast(i64),
    AtMost(i64),
    Exactly(i64),
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
            // Every member must export the same binding; otherwise the body
            // cannot name one amount for all occurrences.
            Self::Compound(members) => {
                let first = members.first()?.numeric_binding()?;
                members
                    .iter()
                    .all(|member| member.numeric_binding() == Some(first))
                    .then_some(first)
            }
            Self::Conditioned { event, .. } => event.numeric_binding(),
            Self::Local(
                LocalTriggerEventNode::CombatDamageToPlayer { amount, .. }
                | LocalTriggerEventNode::SourceDealtDamage { amount, .. },
            ) => Some(*amount),
            // The runtime exports the dealt amount of every dealt-damage
            // observer, the attached permanent's combat damage to a player
            // included (the same combat-to-player publication).
            Self::Local(
                LocalTriggerEventNode::DamageDealt { .. }
                | LocalTriggerEventNode::AttachedObjectDealtDamage { .. }
                | LocalTriggerEventNode::AttachedObjectEvent {
                    action: super::AttachedObjectEventAction::DealCombatDamageToPlayer,
                    ..
                },
            )
            | Self::Object(super::ObjectEventNode {
                verb: super::ObjectEventVerb::DealDamage,
                ..
            }) => Some(EventNumericBinding::DamageAmount),
            // A per-object `deals combat damage to a player` lowers through
            // the same dealt-damage observer (one occurrence per source); a
            // `one or more` batch is a different observer and exports none.
            Self::Object(super::ObjectEventNode {
                verb: super::ObjectEventVerb::DealCombatDamageToPlayer,
                subject,
                ..
            }) if !matches!(
                subject.multiplicity,
                super::EventMultiplicity::PerEvent { .. }
            ) =>
            {
                Some(EventNumericBinding::DamageAmount)
            }
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
            // The runtime records the amount of every life-gain and
            // life-loss occurrence it reports to an observer (CR 119.3,
            // 119.10); one action per event keeps that amount unambiguous.
            Self::Local(LocalTriggerEventNode::PlayerEvent { actions, .. })
                if matches!(actions.as_slice(), [super::PlayerEventAction::GainLife]) =>
            {
                Some(EventNumericBinding::LifeGained)
            }
            Self::Local(LocalTriggerEventNode::PlayerEvent { actions, .. })
                if matches!(actions.as_slice(), [super::PlayerEventAction::LoseLife]) =>
            {
                Some(EventNumericBinding::LifeLost)
            }
            // The counters one placement put (CR 122.6).
            Self::CounterPlacement(_) => Some(EventNumericBinding::CountersPut),
            // `one or more CREATURES you control attack`: the attackers the
            // one batch occurrence matched (CR 508.1, 603.2c).
            Self::Object(super::ObjectEventNode {
                verb: super::ObjectEventVerb::Attack,
                subject,
                ..
            }) if matches!(
                subject.multiplicity,
                super::EventMultiplicity::PerEvent { .. }
            ) && subject.query.controller
                == Some(super::ObjectControllerScope::AbilityController) =>
            {
                Some(EventNumericBinding::EventObjectCount)
            }
            _ => None,
        }
    }

    /// `When you cast this spell` (CR 601.2i): the ability functions while
    /// its object is a spell on the stack (CR 113.6), whatever the card's
    /// types; the cast spell is still the ability's source.
    pub(in crate::oracle::canonical) fn functions_on_the_stack(&self) -> bool {
        match self {
            Self::SpellCast(event) => event.source_spell,
            Self::Conditioned { event, .. } => event.functions_on_the_stack(),
            _ => false,
        }
    }

    /// A cycling trigger functions while its card is cycled from the hand
    /// (CR 702.29), so any card type may own it. Every other event here is
    /// observed by a permanent on the battlefield (`functions_on_the_stack`
    /// names the spell's own cast).
    pub(in crate::oracle::canonical) fn requires_permanent_face(&self) -> bool {
        match self {
            Self::Compound(members) => members.iter().any(Self::requires_permanent_face),
            Self::Conditioned { event, .. } => event.requires_permanent_face(),
            _ => !matches!(
                self,
                Self::Local(LocalTriggerEventNode::SourceCycled { .. })
            ),
        }
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
    /// The event-time condition of a single occurrence, hoisted from the
    /// clause: the ability's `eventCondition` (checked as the event occurs).
    pub event_condition: Option<super::EventTimeConditionNode>,
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
    // A single occurrence's event-time condition belongs to the ability; a
    // compound member keeps its own (only that member's occurrences test it).
    let (event, event_condition) = match event {
        GenericTriggerEventNode::Conditioned { event, condition } => (*event, Some(condition)),
        event => (event, None),
    };
    GenericTriggerEnvelopeAttempt::Recognized(GenericTriggerEnvelopeNode {
        event,
        event_condition,
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
    // The occurrence's own children (a spell's targets, a sacrificed
    // object) may name the source by its printed name: the ability's name
    // links are installed for them, as for a permanent subject.
    let previous = cursor.replace_source_names(Some(name_links.into()));
    let clause = super::read_event_clause(cursor, name_links, name_failure, ",");
    cursor.replace_source_names(previous);
    match clause {
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

/// Whether the installed name links name the current card or face at the
/// cursor (a lexical probe; `recognize_source_reference` reads it).
pub(in crate::oracle::canonical) fn current_source_name_at(cursor: &OracleTokenCursor<'_>) -> bool {
    let position = cursor.position();
    cursor.source_names().iter().any(|link| {
        link.token_start == position
            && matches!(
                link.kind,
                crate::oracle::syntax::NameLinkKind::CurrentCard
                    | crate::oracle::syntax::NameLinkKind::CurrentFace
            )
    })
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
