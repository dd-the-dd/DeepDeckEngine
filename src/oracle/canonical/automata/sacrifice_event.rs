use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ArgumentFrame, EventActorScope, ObjectQueryNode, VerbAgreement, VerbLemma,
    recognize_operand_verb, try_recognize_event_actor_scope,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SacrificeEventNode {
    pub actor: EventActorScope,
    /// The sacrificed-object query; `None` when the event names only the
    /// ability source (`When you sacrifice this Aura`).
    pub object: Option<ObjectQueryNode>,
    pub exclude_source: bool,
    /// `this PERMANENT` / its printed name, alone or as the first
    /// alternative of `SOURCE or another QUERY` (CR 701.21a: only a
    /// permanent's controller sacrifices it).
    pub source: bool,
    /// `one or more QUERY`: one occurrence per sacrifice event with at
    /// least one matching permanent (CR 603.2c).
    pub one_or_more: bool,
}

pub(in crate::oracle::canonical) enum SacrificeEventAttempt {
    NotClaimed,
    Recognized(SacrificeEventNode),
    Failed(MachineFailure),
}

#[derive(Clone, Copy)]
enum SacrificeEventState {
    Trigger,
    Actor,
    Action,
    Determiner,
    Separator,
    Complete,
}

#[derive(Default)]
struct SacrificeEventFrame {
    actor: Option<EventActorScope>,
    object: Option<ObjectQueryNode>,
    exclude_source: bool,
    source: bool,
    one_or_more: bool,
}

/// `this PERMANENT` or the source's printed name starts the sacrificed
/// object (a lexical probe).
fn source_head_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        || super::current_source_name_at(cursor)
}

/// `SOURCE [or another QUERY]` after the sacrifice verb: the ability source
/// itself, and optionally the other permanents of a query with the source
/// excluded (`Whenever you sacrifice this artifact or another artifact`).
fn recognize_source_head(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &mut SacrificeEventFrame,
) -> MachineResult<()> {
    cursor.enter("event.sacrifice.source");
    let names = cursor.source_names().to_vec();
    let source = super::recognize_source_reference(cursor, &names, None)?;
    cursor.require(
        source == super::LocalObjectReference::SourceSelf,
        "the current source as the sacrificed object",
    )?;
    frame.source = true;
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(0, "or") && word(1, "another") {
        cursor.enter("event.sacrifice.source-union");
        cursor.consume("or")?;
        let (object, exclude_source) = super::recognize_permanent_event_object(
            cursor,
            "event.sacrifice.object-head",
            "event.sacrifice.object-domain",
            "event.sacrifice.object-controller",
        )?;
        frame.object = Some(object);
        frame.exclude_source = exclude_source;
    }
    Ok(())
}

/// `one or more [other] QUERY` after the sacrifice verb: the shared
/// quantified subject, read as one occurrence per sacrifice event.
fn recognize_batch_head(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &mut SacrificeEventFrame,
) -> MachineResult<()> {
    let subject = super::recognize_quantified_event_subject(cursor)?;
    cursor.enter("event.sacrifice.batch");
    cursor.require(
        subject.multiplicity == super::EventMultiplicity::PerEvent { minimum: 1 },
        "a `one or more` sacrifice batch",
    )?;
    cursor.require(
        subject
            .query
            .predicate
            .supports_domain(super::ObjectKind::Permanent)
            && subject.query.predicate.has_nominal_head()
            && subject.query.controller.is_none(),
        "a permanent query with a nominal head and no second controller relation",
    )?;
    frame.object = Some(subject.query);
    frame.exclude_source = subject.exclude_source;
    frame.one_or_more = true;
    Ok(())
}

pub(in crate::oracle::canonical) fn try_recognize_sacrifice_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> SacrificeEventAttempt {
    let checkpoint = cursor.checkpoint();
    match recognize_sacrifice_event_attempt(cursor, SacrificeEventState::Trigger) {
        Ok(Some(event)) => SacrificeEventAttempt::Recognized(event),
        Ok(None) => {
            cursor.rollback(checkpoint);
            SacrificeEventAttempt::NotClaimed
        }
        Err(failure) => SacrificeEventAttempt::Failed(failure),
    }
}

/// The sacrifice clause after the trigger word, for the shared event clause
/// (`event_clause.rs`), which owns the trigger word and separator.
pub(in crate::oracle::canonical) fn try_recognize_sacrifice_event_clause(
    cursor: &mut OracleTokenCursor<'_>,
) -> SacrificeEventAttempt {
    let checkpoint = cursor.checkpoint();
    match recognize_sacrifice_event_attempt(cursor, SacrificeEventState::Actor) {
        Ok(Some(event)) => SacrificeEventAttempt::Recognized(event),
        Ok(None) => {
            cursor.rollback(checkpoint);
            SacrificeEventAttempt::NotClaimed
        }
        Err(failure) => SacrificeEventAttempt::Failed(failure),
    }
}

/// From `Trigger`, the complete envelope (trigger word to separator); from
/// `Actor`, the clause alone, completed before the separator.
fn recognize_sacrifice_event_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    start: SacrificeEventState,
) -> MachineResult<Option<SacrificeEventNode>> {
    let with_envelope = matches!(start, SacrificeEventState::Trigger);
    let mut state = start;
    let mut frame = SacrificeEventFrame::default();
    loop {
        state = match state {
            SacrificeEventState::Trigger => {
                cursor.enter("event.sacrifice.trigger");
                let Some(raw) = cursor.peek().filter(|raw| {
                    raw.eq_ignore_ascii_case("when") || raw.eq_ignore_ascii_case("whenever")
                }) else {
                    return Ok(None);
                };
                let word = raw.to_string();
                cursor.consume(&word)?;
                SacrificeEventState::Actor
            }
            SacrificeEventState::Actor => {
                frame.actor = try_recognize_event_actor_scope(cursor)?;
                if frame.actor.is_none() {
                    return Ok(None);
                }
                SacrificeEventState::Action
            }
            SacrificeEventState::Action => {
                // The lexicon verb in the actor's agreement (shared clause).
                cursor.enter("event.sacrifice.action");
                let agreement = if frame.actor == Some(EventActorScope::Controller) {
                    VerbAgreement::Base
                } else {
                    VerbAgreement::ThirdPersonSingular
                };
                if recognize_operand_verb(
                    cursor,
                    agreement,
                    &[VerbLemma::Sacrifice],
                    &[ArgumentFrame::ObjectSelection],
                )?
                .is_none()
                {
                    return Ok(None);
                }
                SacrificeEventState::Determiner
            }
            // The clause form (the shared event clause) also reads the source
            // head and the `one or more` batch; the historical envelope keeps
            // its singular object reader.
            SacrificeEventState::Determiner if !with_envelope && source_head_starts(cursor) => {
                recognize_source_head(cursor, &mut frame)?;
                SacrificeEventState::Complete
            }
            SacrificeEventState::Determiner
                if !with_envelope && super::quantified_plural_subject_starts(cursor) =>
            {
                recognize_batch_head(cursor, &mut frame)?;
                SacrificeEventState::Complete
            }
            SacrificeEventState::Determiner => {
                let (object, exclude_source) = super::recognize_permanent_event_object(
                    cursor,
                    "event.sacrifice.object-head",
                    "event.sacrifice.object-domain",
                    "event.sacrifice.object-controller",
                )?;
                frame.object = Some(object);
                frame.exclude_source = exclude_source;
                if with_envelope {
                    SacrificeEventState::Separator
                } else {
                    SacrificeEventState::Complete
                }
            }
            SacrificeEventState::Separator => {
                cursor.enter("event.sacrifice.separator");
                cursor.consume(",")?;
                SacrificeEventState::Complete
            }
            SacrificeEventState::Complete => {
                cursor.enter("event.sacrifice.complete");
                cursor.require(
                    frame.source || frame.object.is_some(),
                    "the sacrificed object query",
                )?;
                return Ok(Some(SacrificeEventNode {
                    actor: cursor.require_value(frame.actor, "the event actor")?,
                    object: frame.object,
                    exclude_source: frame.exclude_source,
                    source: frame.source,
                    one_or_more: frame.one_or_more,
                }));
            }
        };
    }
}
