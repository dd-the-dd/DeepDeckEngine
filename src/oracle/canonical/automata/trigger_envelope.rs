//! One typed event envelope for the shared triggered-program parent.
//! The following guard, permission and effect tokens remain parent-owned.

use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    EventNumericBinding, LocalObjectReference, LocalTriggerEventNode, ObjectControllerScope,
    PermanentDiedEventBranchNode, PermanentDiedEventNode, SacrificeEventAttempt,
    SacrificeEventNode, SourceTriggerEnvelopeAttempt, SourceTriggerEnvelopeNode,
    SourceTriggerEventKind, SpellCastEventAttempt, SpellCastEventNode, StepEventNode,
    TypedObservedEventObject, recognize_card_drawn_event, recognize_combat_damage_to_player_event,
    recognize_land_played_event, recognize_ordinal_card_drawn_event,
    recognize_permanent_transition_subject, recognize_player_event,
    recognize_source_attacked_event, recognize_source_dealt_damage_event, recognize_step_event,
    try_recognize_sacrifice_event, try_recognize_source_trigger_envelope,
    try_recognize_spell_cast_event,
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
    },
    Local(LocalTriggerEventNode),
    /// Shared quantified subject + event verb (`one or more cards leave …`).
    Object(super::ObjectEventNode),
    /// The ability source with one or more event verbs (`attacks or blocks`).
    SourceEvent(super::SourceEventUnionNode),
    /// Counters put on a permanent (CR 122.6).
    CounterPlacement(super::CounterPlacementEventNode),
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

/// A failure from one event alternative. The envelope keeps the alternative
/// that read furthest as the precise diagnostic of a claimed trigger.
fn keep_furthest(best: &mut Option<MachineFailure>, failure: MachineFailure) {
    if best
        .as_ref()
        .is_none_or(|current| failure.token_index > current.token_index)
    {
        *best = Some(failure);
    }
}

fn is_trigger_introducer(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        ["at", "when", "whenever"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
    })
}

/// Every successful child returns immediately after the envelope's comma.
/// In particular, this reader never consumes `if` or `you may` from the body.
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
    let claimed = is_trigger_introducer(cursor);
    if !claimed && let Some(failure) = unclassified_label_before_trigger(cursor) {
        return GenericTriggerEnvelopeAttempt::FailedCommitted(failure);
    }
    match recognize_event_alternatives(cursor, name_links, name_failure, has_ability_word) {
        GenericTriggerEnvelopeAttempt::RejectedCandidate(failure) if claimed => {
            GenericTriggerEnvelopeAttempt::FailedCommitted(failure)
        }
        GenericTriggerEnvelopeAttempt::NotClaimed if claimed => {
            cursor.enter("trigger.envelope.event");
            GenericTriggerEnvelopeAttempt::FailedCommitted(
                cursor.fail("a supported trigger event after the trigger word"),
            )
        }
        GenericTriggerEnvelopeAttempt::NotClaimed => {
            cursor.rollback(original);
            GenericTriggerEnvelopeAttempt::NotClaimed
        }
        outcome => outcome,
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

fn recognize_event_alternatives(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    has_ability_word: bool,
) -> GenericTriggerEnvelopeAttempt {
    let checkpoint = cursor.checkpoint();

    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("at"))
    {
        cursor.enter("trigger.envelope.step-prefix");
        if let Err(failure) = cursor.consume_phrase(&["at", "the", "beginning", "of"]) {
            return GenericTriggerEnvelopeAttempt::RejectedCandidate(failure);
        }
        let event = match recognize_step_event(cursor) {
            Ok(event) => event,
            Err(failure) => return GenericTriggerEnvelopeAttempt::RejectedCandidate(failure),
        };
        cursor.enter("trigger.envelope.separator");
        return match cursor.consume(",") {
            Ok(()) => recognized(
                cursor,
                GenericTriggerEventNode::Step(event),
                has_ability_word,
            ),
            Err(failure) => GenericTriggerEnvelopeAttempt::FailedCommitted(failure),
        };
    }

    // Rejected alternatives roll back; the furthest one is the diagnostic.
    let mut furthest = None;
    match try_recognize_source_trigger_envelope(cursor, name_links, name_failure) {
        SourceTriggerEnvelopeAttempt::Recognized(event) => {
            return recognized(
                cursor,
                GenericTriggerEventNode::SourceTransition(event),
                has_ability_word,
            );
        }
        SourceTriggerEnvelopeAttempt::Unpromoted(failure) => {
            keep_furthest(&mut furthest, failure);
            cursor.rollback(checkpoint);
        }
        SourceTriggerEnvelopeAttempt::NotClaimed => cursor.rollback(checkpoint),
    }
    match try_recognize_spell_cast_event(cursor) {
        SpellCastEventAttempt::Recognized(event) => {
            return recognized(
                cursor,
                GenericTriggerEventNode::SpellCast(event),
                has_ability_word,
            );
        }
        SpellCastEventAttempt::Unpromoted(failure) => {
            keep_furthest(&mut furthest, failure);
            cursor.rollback(checkpoint);
        }
        SpellCastEventAttempt::NotClaimed => cursor.rollback(checkpoint),
    }
    match super::try_recognize_actor_object_event(cursor) {
        super::ActorObjectEventAttempt::Recognized(event) => {
            return recognized(
                cursor,
                GenericTriggerEventNode::ActorObjectAlternatives(event),
                has_ability_word,
            );
        }
        super::ActorObjectEventAttempt::Rejected(failure) => {
            keep_furthest(&mut furthest, failure);
            cursor.rollback(checkpoint);
        }
        super::ActorObjectEventAttempt::NotClaimed => cursor.rollback(checkpoint),
    }
    match try_recognize_sacrifice_event(cursor) {
        SacrificeEventAttempt::Recognized(event) => {
            return recognized(
                cursor,
                GenericTriggerEventNode::Sacrificed(event),
                has_ability_word,
            );
        }
        SacrificeEventAttempt::Failed(failure) => {
            keep_furthest(&mut furthest, failure);
            cursor.rollback(checkpoint);
        }
        SacrificeEventAttempt::NotClaimed => cursor.rollback(checkpoint),
    }

    cursor.enter("trigger.envelope.introducer");
    let Some(word) = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("when") || raw.eq_ignore_ascii_case("whenever"))
    else {
        return match furthest {
            Some(failure) => GenericTriggerEnvelopeAttempt::RejectedCandidate(failure),
            None => GenericTriggerEnvelopeAttempt::NotClaimed,
        };
    };
    let word = word.to_string();
    if let Err(failure) = cursor.consume(&word) {
        return GenericTriggerEnvelopeAttempt::RejectedCandidate(failure);
    }
    let event_start = cursor.checkpoint();
    // The source subject with one or more event verbs (`attacks or blocks`).
    // A complete union must reach the envelope comma to be claimed here.
    match super::recognize_source_event_union(cursor, name_links, name_failure) {
        Ok(event) => {
            cursor.enter("trigger.envelope.separator");
            match cursor.consume(",") {
                Ok(()) => {
                    // A lone `attacks` keeps its established event atom.
                    let event = if event.verbs == [super::SourceEventVerb::Attacked]
                        && event.modifiers.is_empty()
                    {
                        GenericTriggerEventNode::Local(LocalTriggerEventNode::SourceAttacked {
                            source: event.source,
                        })
                    } else {
                        GenericTriggerEventNode::SourceEvent(event)
                    };
                    return recognized(cursor, event, has_ability_word);
                }
                Err(failure) => keep_furthest(&mut furthest, failure),
            }
        }
        Err(failure) => keep_furthest(&mut furthest, failure),
    }
    cursor.rollback(event_start);

    match recognize_permanent_transition_subject(cursor, name_links, name_failure) {
        Ok(mut subjects) => {
            let kind = match cursor.peek() {
                Some(raw) if raw.eq_ignore_ascii_case("enters") => {
                    Some(PermanentTransitionKind::Entered)
                }
                Some(raw) if raw.eq_ignore_ascii_case("dies") => {
                    Some(PermanentTransitionKind::Died)
                }
                _ => None,
            };
            if let Some(kind) = kind {
                return finish_permanent_transition(cursor, kind, &mut subjects, has_ability_word);
            }
        }
        Err(failure) => keep_furthest(&mut furthest, failure),
    }
    cursor.rollback(event_start);

    // Local event atoms do not own their trigger word, comma or body. Their
    // typed result is the commit boundary; an Err is a candidate mismatch.
    let local_readers: &[fn(
        &mut OracleTokenCursor<'_>,
        &[ResolvedNameLink],
        Option<&NameResolutionFailure>,
    ) -> MachineResult<LocalTriggerEventNode>] = &[
        recognize_source_attacked_event,
        recognize_combat_damage_to_player_event,
        recognize_source_dealt_damage_event,
        super::recognize_attached_object_event,
    ];
    for reader in local_readers {
        match reader(cursor, name_links, name_failure) {
            Ok(event) => return finish_local(cursor, event, has_ability_word),
            Err(failure) => keep_furthest(&mut furthest, failure),
        }
        cursor.rollback(event_start);
    }
    let player_readers: &[fn(
        &mut OracleTokenCursor<'_>,
    ) -> MachineResult<LocalTriggerEventNode>] = &[
        recognize_ordinal_card_drawn_event,
        recognize_card_drawn_event,
        recognize_land_played_event,
        recognize_player_event,
    ];
    for reader in player_readers {
        match reader(cursor) {
            Ok(event) => return finish_local(cursor, event, has_ability_word),
            Err(failure) => keep_furthest(&mut furthest, failure),
        }
        cursor.rollback(event_start);
    }
    // `you cycle SOURCE`: the player actor with the source as object.
    match super::recognize_source_cycled_event(cursor, name_links, name_failure) {
        Ok(event) => return finish_local(cursor, event, has_ability_word),
        Err(failure) => keep_furthest(&mut furthest, failure),
    }
    cursor.rollback(event_start);
    // `one or more COUNTERS are put on ...` reads counters, not a plural
    // object subject, so it is tried before the object-event frame.
    match super::recognize_counter_placement_event(cursor, name_links, name_failure) {
        Ok(event) => {
            cursor.enter("trigger.envelope.separator");
            return match cursor.consume(",") {
                Ok(()) => recognized(
                    cursor,
                    GenericTriggerEventNode::CounterPlacement(event),
                    has_ability_word,
                ),
                Err(failure) => GenericTriggerEnvelopeAttempt::FailedCommitted(failure),
            };
        }
        Err(failure) => keep_furthest(&mut furthest, failure),
    }
    cursor.rollback(event_start);
    // A plural `N or more` subject commits to the shared object-event frame;
    // a singular subject is claimed only when its verb frame is complete.
    let plural = super::quantified_plural_subject_starts(cursor);
    match super::recognize_object_event(cursor) {
        Ok(event) => {
            cursor.enter("trigger.envelope.separator");
            return match cursor.consume(",") {
                Ok(()) => recognized(
                    cursor,
                    GenericTriggerEventNode::Object(event),
                    has_ability_word,
                ),
                Err(failure) => GenericTriggerEnvelopeAttempt::FailedCommitted(failure),
            };
        }
        Err(failure) if plural => return GenericTriggerEnvelopeAttempt::FailedCommitted(failure),
        Err(failure) => {
            keep_furthest(&mut furthest, failure);
            cursor.rollback(event_start);
        }
    }
    match furthest {
        Some(failure) => GenericTriggerEnvelopeAttempt::RejectedCandidate(failure),
        None => GenericTriggerEnvelopeAttempt::NotClaimed,
    }
}

fn finish_permanent_transition(
    cursor: &mut OracleTokenCursor<'_>,
    kind: PermanentTransitionKind,
    subjects: &mut PermanentDiedEventNode,
    has_ability_word: bool,
) -> GenericTriggerEnvelopeAttempt {
    cursor.enter("trigger.envelope.permanent-transition");
    let action = match kind {
        PermanentTransitionKind::Entered => "enters",
        PermanentTransitionKind::Died => "dies",
    };
    if let Err(failure) = cursor.consume(action) {
        return GenericTriggerEnvelopeAttempt::FailedCommitted(failure);
    }
    let mut entry_state = None;
    if kind == PermanentTransitionKind::Entered {
        cursor.enter("trigger.envelope.permanent-subject");
        if subjects.alternatives.iter().any(|branch| {
            matches!(branch, PermanentDiedEventBranchNode::Filtered { predicate, .. }
                if !predicate.has_nominal_head())
        }) {
            return GenericTriggerEnvelopeAttempt::FailedCommitted(
                cursor.fail("an entering object subject with a nominal head"),
            );
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        {
            if let Err(failure) = cursor.consume_phrase(&["the", "battlefield"]) {
                return GenericTriggerEnvelopeAttempt::FailedCommitted(failure);
            }
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("under"))
        {
            let scope = match recognize_entering_controller(cursor) {
                Ok(scope) => scope,
                Err(failure) => {
                    return GenericTriggerEnvelopeAttempt::FailedCommitted(failure);
                }
            };
            for branch in &mut subjects.alternatives {
                let PermanentDiedEventBranchNode::Filtered { controller, .. } = branch else {
                    return GenericTriggerEnvelopeAttempt::FailedCommitted(
                        cursor
                            .fail("an entering controller qualifier on a filtered object subject"),
                    );
                };
                if controller.is_some() {
                    return GenericTriggerEnvelopeAttempt::FailedCommitted(
                        cursor.fail("one controller relation per event subject"),
                    );
                }
                *controller = Some(scope);
            }
        }
        entry_state = match cursor.peek() {
            Some(word) if word.eq_ignore_ascii_case("tapped") => Some(EntryStatePredicate::Tapped),
            Some(word) if word.eq_ignore_ascii_case("untapped") => {
                Some(EntryStatePredicate::Untapped)
            }
            _ => None,
        };
        if entry_state.is_some() {
            cursor.enter("trigger.envelope.entry-state");
            if let Err(failure) = cursor.advance_to(cursor.position() + 1) {
                return GenericTriggerEnvelopeAttempt::FailedCommitted(failure);
            }
        }
    }
    cursor.enter("trigger.envelope.separator");
    match cursor.consume(",") {
        Ok(()) => recognized(
            cursor,
            GenericTriggerEventNode::PermanentTransition {
                kind,
                subjects: subjects.clone(),
                entry_state,
            },
            has_ability_word,
        ),
        Err(failure) => GenericTriggerEnvelopeAttempt::FailedCommitted(failure),
    }
}

fn finish_local(
    cursor: &mut OracleTokenCursor<'_>,
    event: LocalTriggerEventNode,
    has_ability_word: bool,
) -> GenericTriggerEnvelopeAttempt {
    cursor.enter("trigger.envelope.separator");
    match cursor.consume(",") {
        Ok(()) => recognized(
            cursor,
            GenericTriggerEventNode::Local(event),
            has_ability_word,
        ),
        Err(failure) => GenericTriggerEnvelopeAttempt::FailedCommitted(failure),
    }
}

/// Post-event control grammar; ObjectQuery continues to own pre-event
/// controller qualifiers. Combining the two controller scopes is rejected.
fn recognize_entering_controller(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectControllerScope> {
    cursor.enter("event.entering-controller.preposition");
    cursor.consume("under")?;
    cursor.enter("event.entering-controller.player");
    let scope = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        cursor.consume("your")?;
        ObjectControllerScope::AbilityController
    } else {
        cursor.consume("an")?;
        let possessive = cursor
            .peek()
            .filter(|raw| {
                raw.eq_ignore_ascii_case("opponent's") || raw.eq_ignore_ascii_case("opponent’s")
            })
            .ok_or_else(|| cursor.fail("a represented entering-controller player scope"))?
            .to_string();
        cursor.consume(&possessive)?;
        ObjectControllerScope::OpponentOfAbilityController
    };
    cursor.enter("event.entering-controller.relation");
    cursor.consume("control")?;
    Ok(scope)
}

#[cfg(test)]
#[path = "trigger_envelope_tests.rs"]
mod tests;
