//! The one event clause reader (S5, docs/oracle-event-model.md).
//!
//! ```text
//! TriggerCondition := EventClause ([","] "and" EventClause)* BOUNDARY
//! EventClause := "at the beginning of" Step [EventTime]
//!              | ("when" | "whenever") Occurrence [ModifierTail] [EventTime]
//! EventTime   := "while" CONDITION          -- `event_modifiers.rs`
//! Occurrence  := SUBJECT LEMMA-FRAME
//!   SUBJECT    the source, a quantified object query, a player actor, an
//!              attached object, a targeted-object union
//!   LEMMA      a verb lexicon lemma in the subject's agreement
//!   FRAME      active, passive (`is put into`, `is dealt`), become-STATE,
//!              targeting (`becomes the target of`)
//! ModifierTail := `event_modifiers.rs`
//! ```
//!
//! The clause reader owns the trigger word and the boundary (the comma of a
//! triggered ability, CR 603.1). Its children read only the occurrence and
//! never consume either; each child is one typed alternative. A child that
//! fails rolls back and the alternative that read furthest becomes the
//! diagnostic of the claimed clause. The boundary is a parameter so that an
//! embedded trigger (CR 603.7 delayed, 603.12 reflexive) reads its event with
//! the same reader.
//!
//! Modifiers of the shared tail are absorbed by the subject kinds that type
//! them (the source union, quantified objects). A turn owner that no pattern
//! matcher evaluates is a state of the event-time condition; any other
//! unevaluated modifier is an engine gap, never dropped.
//!
//! A trigger condition list (CR 603.2, `When this creature enters and at the
//! beginning of your upkeep, ...`) is read here: each member keeps its own
//! introducer, subject and frame, and the list owns the `and` between two
//! complete clauses. `enters and attacks` (no introducer after `and`) is
//! not a list.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::machine::MachineCheckpoint;
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ActorObjectEventAttempt, EntryStatePredicate, EventModifierTail, EventMultiplicity,
    EventTimeConditionNode, GenericTriggerEventNode, LocalTriggerEventNode, ObjectControllerScope,
    ObjectEventVerb, PermanentDiedEventBranchNode, PermanentDiedEventNode, PermanentTransitionKind,
    SacrificeEventAttempt, SourceEventModifier, SourceEventVerb, SourceTriggerEnvelopeAttempt,
    SpellCastOrdinalSupport, recognize_card_drawn_event, recognize_combat_damage_to_player_event,
    recognize_land_played_event, recognize_optional_spell_cast_event_atom,
    recognize_ordinal_card_drawn_event, recognize_permanent_transition_subject,
    recognize_player_event, recognize_source_attacked_event, recognize_source_dealt_damage_event,
    recognize_step_event, try_recognize_actor_object_event_clause,
    try_recognize_sacrifice_event_clause, try_recognize_source_trigger_event,
};

/// The structural introducer of a trigger condition (CR 603.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EventIntroducer {
    /// `at the beginning of STEP` (CR 603.2b, 503.1).
    At,
    When,
    Whenever,
}

/// The transactional outcome of one event clause.
pub(in crate::oracle::canonical) enum EventClauseAttempt {
    /// No introducer at the cursor; the cursor is unchanged.
    NotClaimed,
    /// The introducer was read but no alternative is complete; the failure
    /// is the alternative that read furthest.
    Rejected(MachineFailure),
    /// An alternative committed and then failed.
    Committed(MachineFailure),
    /// A complete occurrence; the cursor is after the boundary.
    Recognized(GenericTriggerEventNode),
}

/// The trigger introducer at the cursor, without consuming it.
pub(in crate::oracle::canonical) fn event_introducer_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<EventIntroducer> {
    let raw = cursor.peek()?;
    if raw.eq_ignore_ascii_case("at") {
        Some(EventIntroducer::At)
    } else if raw.eq_ignore_ascii_case("when") {
        Some(EventIntroducer::When)
    } else if raw.eq_ignore_ascii_case("whenever") {
        Some(EventIntroducer::Whenever)
    } else {
        None
    }
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// `[,] and INTRODUCER` continues a trigger condition list (CR 603.2): the
/// next member is a complete clause with its own trigger word (`when`,
/// `whenever` or the complete `at the beginning of`; `and at least` is no
/// list). A lexical probe that commits nothing.
fn compound_continuation_at(cursor: &OracleTokenCursor<'_>) -> bool {
    let offset = usize::from(cursor.peek() == Some(","));
    word_at(cursor, offset, "and")
        && (word_at(cursor, offset + 1, "when")
            || word_at(cursor, offset + 1, "whenever")
            || (word_at(cursor, offset + 1, "at")
                && word_at(cursor, offset + 2, "the")
                && word_at(cursor, offset + 3, "beginning")
                && word_at(cursor, offset + 4, "of")))
}

/// Whether a child alternative owns the clause once it is complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundaryPolicy {
    /// A missing boundary rolls back to try the next alternative.
    Backtrack,
    /// A missing boundary is the clause's committed failure.
    Commit,
}

/// One clause reading: the occurrence start and the furthest failure.
struct ClauseReading<'b> {
    start: MachineCheckpoint,
    furthest: Option<MachineFailure>,
    boundary: &'b str,
    name_links: &'b [ResolvedNameLink],
    name_failure: Option<&'b NameResolutionFailure>,
}

impl ClauseReading<'_> {
    /// A failure from one event alternative. The clause keeps the one that
    /// read furthest as the precise diagnostic of a claimed trigger.
    fn keep_furthest(&mut self, failure: MachineFailure) {
        if self
            .furthest
            .as_ref()
            .is_none_or(|current| failure.token_index > current.token_index)
        {
            self.furthest = Some(failure);
        }
    }

    fn reject(&mut self, cursor: &mut OracleTokenCursor<'_>, failure: MachineFailure) {
        self.keep_furthest(failure);
        cursor.rollback(self.start);
    }

    /// Closes an alternative: its modifier tail, then the boundary. `None`
    /// lets the next alternative run.
    fn finish(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        event: GenericTriggerEventNode,
        policy: BoundaryPolicy,
    ) -> Option<EventClauseAttempt> {
        match close_occurrence(
            cursor,
            event,
            self.boundary,
            self.name_links,
            self.name_failure,
        ) {
            Ok(event) => Some(EventClauseAttempt::Recognized(event)),
            Err(failure) if policy == BoundaryPolicy::Commit => {
                Some(EventClauseAttempt::Committed(failure))
            }
            Err(failure) => {
                self.reject(cursor, failure);
                None
            }
        }
    }

    fn unclaimed(self, cursor: &mut OracleTokenCursor<'_>) -> EventClauseAttempt {
        match self.furthest {
            Some(failure) => EventClauseAttempt::Rejected(failure),
            None => {
                cursor.enter("trigger.envelope.event");
                EventClauseAttempt::Rejected(
                    cursor.fail("a supported trigger event after the trigger word"),
                )
            }
        }
    }
}

/// The shared modifier tail of a completed occurrence, its event-time
/// condition, then the boundary. Before `[,] and INTRODUCER` the boundary
/// belongs to the last member of the trigger condition list.
fn close_occurrence(
    cursor: &mut OracleTokenCursor<'_>,
    event: GenericTriggerEventNode,
    boundary: &str,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<GenericTriggerEventNode> {
    let tail = super::read_event_modifier_tail(cursor)?;
    let (event, mut time) = absorb_modifiers(cursor, event, tail)?;
    if super::while_condition_starts(cursor) {
        time.condition = Some(super::recognize_while_condition(
            cursor,
            name_links,
            name_failure,
        )?);
    }
    let mut event = if time.is_empty() {
        event
    } else {
        GenericTriggerEventNode::Conditioned {
            event: Box::new(event),
            condition: time,
        }
    };
    if compound_continuation_at(cursor) {
        return Ok(event);
    }
    cursor.enter("trigger.envelope.separator");
    cursor.consume(boundary)?;
    if let GenericTriggerEventNode::SourceTransition(node) = &mut event {
        node.body_start = cursor.position();
    }
    Ok(event)
}

/// Whether the per-object attack pattern evaluates the turn owner of this
/// quantified object event (the other object matchers have no turn field).
fn object_pattern_evaluates_turn_owner(node: &super::ObjectEventNode) -> bool {
    node.verb == ObjectEventVerb::Attack
        && node.subject.multiplicity == EventMultiplicity::PerObject
}

/// The combat verbs whose source pattern evaluates a turn owner.
fn source_pattern_evaluates_turn_owner(verbs: &[SourceEventVerb]) -> bool {
    verbs.iter().all(|verb| {
        matches!(
            verb,
            SourceEventVerb::Attacked | SourceEventVerb::Blocked | SourceEventVerb::BecameBlocked
        )
    })
}

/// The subject kinds that type the clause modifiers absorb them. A turn
/// owner that no pattern matcher evaluates becomes the occurrence's
/// event-time condition (whose turn it is is a state when the event occurs,
/// CR 500.1); any other modifier without a matcher is an engine gap, never
/// dropped.
fn absorb_modifiers(
    cursor: &mut OracleTokenCursor<'_>,
    event: GenericTriggerEventNode,
    tail: EventModifierTail,
) -> MachineResult<(GenericTriggerEventNode, EventTimeConditionNode)> {
    let mut time = EventTimeConditionNode::default();
    match event {
        GenericTriggerEventNode::SourceEvent(mut node) => {
            for modifier in SourceEventModifier::from_tail(tail) {
                cursor.enter("event.modifier.distinct");
                cursor.require(
                    !node.modifiers.contains(&modifier),
                    "each event modifier at most once",
                )?;
                node.modifiers.push(modifier);
            }
            if !source_pattern_evaluates_turn_owner(&node.verbs) {
                // The source union read its own tail: hoist the turn owner
                // that its patterns cannot evaluate.
                node.modifiers.retain(|modifier| match modifier {
                    SourceEventModifier::DuringTurn(owner) => {
                        time.turn_owner = Some(*owner);
                        false
                    }
                    _ => true,
                });
            }
            Ok((GenericTriggerEventNode::SourceEvent(node), time))
        }
        GenericTriggerEventNode::Object(mut node) => {
            cursor.enter("event.modifier.distinct");
            cursor.require(
                !(tail.not_blocked && node.modifiers.not_blocked)
                    && !(tail.first_time_each_turn && node.modifiers.first_time_each_turn)
                    && !(tail.turn_owner.is_some() && node.modifiers.turn_owner.is_some()),
                "each event modifier at most once",
            )?;
            node.modifiers.not_blocked |= tail.not_blocked;
            node.modifiers.first_time_each_turn |= tail.first_time_each_turn;
            node.modifiers.turn_owner = node.modifiers.turn_owner.or(tail.turn_owner);
            if !object_pattern_evaluates_turn_owner(&node) {
                time.turn_owner = node.modifiers.turn_owner.take();
            }
            Ok((GenericTriggerEventNode::Object(node), time))
        }
        _ if tail.not_blocked || tail.first_time_each_turn => {
            // The modifier was read: the missing piece is the runtime
            // matcher (S0 engine gap), never a grammar failure.
            cursor.enter("trigger.event.modifier-matcher");
            Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::EventMatcher,
                &format!(
                    "an engine event matcher for {} on this event",
                    tail.first_modifier_name().unwrap_or("this modifier")
                ),
            ))
        }
        event => {
            time.turn_owner = tail.turn_owner;
            Ok((event, time))
        }
    }
}

/// Reads `INTRODUCER OCCURRENCE [MODIFIERS] BOUNDARY`. Every successful
/// child returns immediately after the boundary; in particular this reader
/// never consumes `if` or `you may` from the body.
///
/// A trigger condition list (`EVENT and when|whenever|at the beginning of
/// EVENT`, CR 603.2) is one `Compound` event: every member is read by this
/// same clause reader and the last one consumes the boundary. Once the
/// first member is complete, a failing member is the committed failure of
/// the list.
pub(in crate::oracle::canonical) fn read_event_clause(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    boundary: &str,
) -> EventClauseAttempt {
    let first = match read_one_event_clause(cursor, name_links, name_failure, boundary) {
        EventClauseAttempt::Recognized(event) => event,
        outcome => return outcome,
    };
    if !compound_continuation_at(cursor) {
        return EventClauseAttempt::Recognized(first);
    }
    let mut members = vec![first];
    while compound_continuation_at(cursor) {
        cursor.enter("trigger.envelope.compound-connector");
        if cursor.peek() == Some(",")
            && let Err(failure) = cursor.consume(",")
        {
            return EventClauseAttempt::Committed(failure);
        }
        if let Err(failure) = cursor.consume("and") {
            return EventClauseAttempt::Committed(failure);
        }
        match read_one_event_clause(cursor, name_links, name_failure, boundary) {
            EventClauseAttempt::Recognized(member) => members.push(member),
            EventClauseAttempt::Rejected(failure) | EventClauseAttempt::Committed(failure) => {
                return EventClauseAttempt::Committed(failure);
            }
            EventClauseAttempt::NotClaimed => {
                cursor.enter("trigger.envelope.compound-member");
                return EventClauseAttempt::Committed(
                    cursor.fail("a complete trigger condition after `and`"),
                );
            }
        }
    }
    EventClauseAttempt::Recognized(GenericTriggerEventNode::Compound(members))
}

/// One member of a trigger condition list: its introducer, occurrence,
/// modifiers, event-time condition and either the list connector (left at
/// the cursor) or the consumed boundary.
fn read_one_event_clause(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    boundary: &str,
) -> EventClauseAttempt {
    match event_introducer_at(cursor) {
        None => EventClauseAttempt::NotClaimed,
        Some(EventIntroducer::At) => read_step_clause(cursor, name_links, name_failure, boundary),
        Some(_) => read_occurrence_clause(cursor, name_links, name_failure, boundary),
    }
}

fn read_step_clause(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    boundary: &str,
) -> EventClauseAttempt {
    cursor.enter("trigger.envelope.step-prefix");
    if let Err(failure) = cursor.consume_phrase(&["at", "the", "beginning", "of"]) {
        return EventClauseAttempt::Rejected(failure);
    }
    let event = match recognize_step_event(cursor) {
        Ok(event) => event,
        Err(failure) => return EventClauseAttempt::Rejected(failure),
    };
    match close_occurrence(
        cursor,
        GenericTriggerEventNode::Step(event),
        boundary,
        name_links,
        name_failure,
    ) {
        Ok(event) => EventClauseAttempt::Recognized(event),
        Err(failure) => EventClauseAttempt::Committed(failure),
    }
}

/// The local event atoms: they do not own their trigger word, boundary or
/// body. Their typed result is the commit boundary; an Err is a candidate
/// mismatch.
type LocalReader = fn(
    &mut OracleTokenCursor<'_>,
    &[ResolvedNameLink],
    Option<&NameResolutionFailure>,
) -> MachineResult<LocalTriggerEventNode>;

type PlayerReader = fn(&mut OracleTokenCursor<'_>) -> MachineResult<LocalTriggerEventNode>;

type AttackDeclarationReader = fn(
    &mut OracleTokenCursor<'_>,
    &[ResolvedNameLink],
    Option<&NameResolutionFailure>,
) -> MachineResult<super::AttackDeclarationNode>;

/// The player attack declaration has no source reference to resolve.
fn player_attack_declaration(
    cursor: &mut OracleTokenCursor<'_>,
    _name_links: &[ResolvedNameLink],
    _name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<super::AttackDeclarationNode> {
    super::recognize_player_attack_declaration(cursor)
}

fn read_occurrence_clause(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    boundary: &str,
) -> EventClauseAttempt {
    cursor.enter("trigger.envelope.introducer");
    let word = cursor.peek().unwrap_or_default().to_string();
    if let Err(failure) = cursor.consume(&word) {
        return EventClauseAttempt::Rejected(failure);
    }
    let mut reading = ClauseReading {
        start: cursor.checkpoint(),
        furthest: None,
        boundary,
        name_links,
        name_failure,
    };

    // The source as the subject of a battlefield transition.
    match try_recognize_source_trigger_event(cursor, name_links, name_failure) {
        SourceTriggerEnvelopeAttempt::Recognized(event) => {
            if let Some(outcome) = reading.finish(
                cursor,
                GenericTriggerEventNode::SourceTransition(event),
                BoundaryPolicy::Backtrack,
            ) {
                return outcome;
            }
        }
        SourceTriggerEnvelopeAttempt::Unpromoted(failure) => reading.reject(cursor, failure),
        SourceTriggerEnvelopeAttempt::NotClaimed => cursor.rollback(reading.start),
    }
    // A player actor with `cast` (CR 601.2i); the cast pattern counts
    // matching spells by the caster this turn.
    match recognize_optional_spell_cast_event_atom(
        cursor,
        SpellCastOrdinalSupport::MatchingSpellOrdinals,
    ) {
        Ok(Some(event)) => {
            if let Some(outcome) = reading.finish(
                cursor,
                GenericTriggerEventNode::SpellCast(event),
                BoundaryPolicy::Backtrack,
            ) {
                return outcome;
            }
        }
        Ok(None) => cursor.rollback(reading.start),
        Err(failure) => reading.reject(cursor, failure),
    }
    // A player actor with alternative verbs sharing one object.
    match try_recognize_actor_object_event_clause(cursor) {
        ActorObjectEventAttempt::Recognized(event) => {
            if let Some(outcome) = reading.finish(
                cursor,
                GenericTriggerEventNode::ActorObjectAlternatives(event),
                BoundaryPolicy::Backtrack,
            ) {
                return outcome;
            }
        }
        ActorObjectEventAttempt::Rejected(failure) => reading.reject(cursor, failure),
        ActorObjectEventAttempt::NotClaimed => cursor.rollback(reading.start),
    }
    match try_recognize_sacrifice_event_clause(cursor) {
        SacrificeEventAttempt::Recognized(event) => {
            if let Some(outcome) = reading.finish(
                cursor,
                GenericTriggerEventNode::Sacrificed(event),
                BoundaryPolicy::Backtrack,
            ) {
                return outcome;
            }
        }
        SacrificeEventAttempt::Failed(failure) => reading.reject(cursor, failure),
        SacrificeEventAttempt::NotClaimed => cursor.rollback(reading.start),
    }
    // The targeting frame of `become` with a targeted-object union subject.
    match super::recognize_became_target_event(cursor) {
        Ok(event) => {
            if let Some(outcome) = reading.finish(
                cursor,
                GenericTriggerEventNode::BecameTarget(event),
                BoundaryPolicy::Backtrack,
            ) {
                return outcome;
            }
        }
        Err(failure) => reading.reject(cursor, failure),
    }
    // The source subject with one or more event verbs (`attacks or blocks`).
    match super::recognize_source_event_union(cursor, name_links, name_failure) {
        Ok(event) => {
            if let Some(outcome) = reading.finish(
                cursor,
                GenericTriggerEventNode::SourceEvent(event),
                BoundaryPolicy::Backtrack,
            ) {
                return match outcome {
                    // A lone `attacks` keeps its established event atom.
                    EventClauseAttempt::Recognized(GenericTriggerEventNode::SourceEvent(event))
                        if event.verbs == [SourceEventVerb::Attacked]
                            && event.modifiers.is_empty() =>
                    {
                        EventClauseAttempt::Recognized(GenericTriggerEventNode::Local(
                            LocalTriggerEventNode::SourceAttacked {
                                source: event.source,
                            },
                        ))
                    }
                    outcome => outcome,
                };
            }
        }
        Err(failure) => reading.reject(cursor, failure),
    }
    // A permanent subject union with a battlefield transition.
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
                return match read_permanent_transition(cursor, kind, &mut subjects) {
                    Ok(event) => reading
                        .finish(cursor, event, BoundaryPolicy::Commit)
                        .expect("a committed alternative always concludes"),
                    Err(failure) => EventClauseAttempt::Committed(failure),
                };
            }
            cursor.rollback(reading.start);
        }
        Err(failure) => reading.reject(cursor, failure),
    }
    // `SOURCE has N or more|no COUNTERS on it`: a state trigger (CR 603.8).
    match super::recognize_source_counter_state_event(cursor, name_links, name_failure) {
        Ok(event) => {
            return reading
                .finish(
                    cursor,
                    GenericTriggerEventNode::SourceCounterState(event),
                    BoundaryPolicy::Commit,
                )
                .expect("a committed alternative always concludes");
        }
        Err(failure) => reading.reject(cursor, failure),
    }
    // One attack declaration (CR 508.1): the source coordinated with other
    // attackers, or a player attacking with a quantified set.
    let attack_declarations: [AttackDeclarationReader; 2] = [
        super::recognize_source_attack_declaration,
        player_attack_declaration,
    ];
    for reader in attack_declarations {
        match reader(cursor, name_links, name_failure) {
            Ok(event) => {
                return reading
                    .finish(
                        cursor,
                        GenericTriggerEventNode::Local(LocalTriggerEventNode::AttackDeclaration(
                            event,
                        )),
                        BoundaryPolicy::Commit,
                    )
                    .expect("a committed alternative always concludes");
            }
            Err(failure) => reading.reject(cursor, failure),
        }
    }
    let local_readers: [LocalReader; 5] = [
        recognize_source_attacked_event,
        recognize_combat_damage_to_player_event,
        recognize_source_dealt_damage_event,
        super::recognize_last_counter_removed_event,
        super::recognize_attached_object_event,
    ];
    let player_readers: [PlayerReader; 4] = [
        recognize_ordinal_card_drawn_event,
        recognize_card_drawn_event,
        recognize_land_played_event,
        recognize_player_event,
    ];
    for reader in local_readers {
        match reader(cursor, name_links, name_failure) {
            Ok(event) => {
                return reading
                    .finish(
                        cursor,
                        GenericTriggerEventNode::Local(event),
                        BoundaryPolicy::Commit,
                    )
                    .expect("a committed alternative always concludes");
            }
            Err(failure) => reading.reject(cursor, failure),
        }
    }
    for reader in player_readers {
        match reader(cursor) {
            Ok(event) => {
                return reading
                    .finish(
                        cursor,
                        GenericTriggerEventNode::Local(event),
                        BoundaryPolicy::Commit,
                    )
                    .expect("a committed alternative always concludes");
            }
            Err(failure) => reading.reject(cursor, failure),
        }
    }
    // `you cycle SOURCE`: the player actor with the source as object, after
    // the player atoms (its established position).
    match super::recognize_source_cycled_event(cursor, name_links, name_failure) {
        Ok(event) => {
            return reading
                .finish(
                    cursor,
                    GenericTriggerEventNode::Local(event),
                    BoundaryPolicy::Commit,
                )
                .expect("a committed alternative always concludes");
        }
        Err(failure) => reading.reject(cursor, failure),
    }
    // `one or more COUNTERS are put on ...` reads counters, not a plural
    // object subject, so it is tried before the object-event frame.
    match super::recognize_counter_placement_event(cursor, name_links, name_failure) {
        Ok(event) => {
            return reading
                .finish(
                    cursor,
                    GenericTriggerEventNode::CounterPlacement(event),
                    BoundaryPolicy::Commit,
                )
                .expect("a committed alternative always concludes");
        }
        Err(failure) => reading.reject(cursor, failure),
    }
    // A plural `N or more` subject commits to the shared object-event frame;
    // a singular subject is claimed only when its verb frame is complete.
    let plural = super::quantified_plural_subject_starts(cursor);
    match super::recognize_object_event(cursor) {
        Ok(event) => {
            return reading
                .finish(
                    cursor,
                    GenericTriggerEventNode::Object(event),
                    BoundaryPolicy::Commit,
                )
                .expect("a committed alternative always concludes");
        }
        Err(failure) if plural => return EventClauseAttempt::Committed(failure),
        Err(failure) => reading.reject(cursor, failure),
    }
    reading.unclaimed(cursor)
}

/// `SUBJECTS enters|dies [the battlefield] [under CONTROLLER control]
/// [tapped|untapped] [or dies|enters]`, after the subject union.
fn read_permanent_transition(
    cursor: &mut OracleTokenCursor<'_>,
    kind: PermanentTransitionKind,
    subjects: &mut PermanentDiedEventNode,
) -> MachineResult<GenericTriggerEventNode> {
    cursor.enter("trigger.envelope.permanent-transition");
    let action = match kind {
        PermanentTransitionKind::Entered => "enters",
        PermanentTransitionKind::Died => "dies",
    };
    cursor.consume(action)?;
    let mut entry_state = None;
    let mut entering_controller = false;
    let mut origin = None;
    if kind == PermanentTransitionKind::Entered {
        cursor.enter("trigger.envelope.permanent-subject");
        if subjects.alternatives.iter().any(|branch| {
            matches!(branch, PermanentDiedEventBranchNode::Filtered { predicate, .. }
                if !predicate.has_nominal_head())
        }) {
            return Err(cursor.fail("an entering object subject with a nominal head"));
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        {
            cursor.consume_phrase(&["the", "battlefield"])?;
        }
        // `from ZONE`: the origin the engine records on the entering
        // object. An owner-relative zone (`your graveyard`) would add an
        // ownership relation the entry matcher does not evaluate.
        if super::entry_origin_starts(cursor) {
            let entry_origin = super::recognize_entry_origin(cursor)?;
            cursor.enter("trigger.envelope.entry-origin-owner");
            cursor.require(
                !entry_origin.own,
                "an entry origin without an owner relation (`a graveyard`, `exile`)",
            )?;
            origin = Some(entry_origin);
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("under"))
        {
            let scope = recognize_entering_controller(cursor)?;
            for branch in &mut subjects.alternatives {
                let PermanentDiedEventBranchNode::Filtered { controller, .. } = branch else {
                    return Err(cursor
                        .fail("an entering controller qualifier on a filtered object subject"));
                };
                if controller.is_some() {
                    return Err(cursor.fail("one controller relation per event subject"));
                }
                *controller = Some(scope);
            }
            entering_controller = true;
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
            cursor.advance_to(cursor.position() + 1)?;
        }
    }
    // `enters or dies` / `dies or enters`: the other transition of the same
    // subjects as an alternative event.
    let mut union_kind = None;
    let other = match kind {
        PermanentTransitionKind::Entered => ("dies", PermanentTransitionKind::Died),
        PermanentTransitionKind::Died => ("enters", PermanentTransitionKind::Entered),
    };
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(other.0))
    {
        cursor.enter("trigger.envelope.transition-union");
        // `under your control` is the entering branch's post-event
        // controller; the subjects carry one pre-event controller relation
        // shared by both branches, so the union would leak it into `or dies`.
        cursor.require(
            !entering_controller && origin.is_none(),
            "an entering-controller scope or origin without a transition union (it binds only the entering branch)",
        )?;
        cursor.consume_phrase(&["or", other.0])?;
        union_kind = Some(other.1);
    }
    Ok(GenericTriggerEventNode::PermanentTransition {
        kind,
        subjects: subjects.clone(),
        entry_state,
        origin,
        union_kind,
    })
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
#[path = "event_clause_reader_tests.rs"]
mod tests;
