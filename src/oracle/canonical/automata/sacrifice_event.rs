use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ArgumentFrame, EventActorScope, ObjectQueryNode, VerbAgreement, VerbLemma,
    recognize_operand_verb, try_recognize_event_actor_scope,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SacrificeEventNode {
    pub actor: EventActorScope,
    pub object: ObjectQueryNode,
    pub exclude_source: bool,
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
                return Ok(Some(SacrificeEventNode {
                    actor: cursor.require_value(frame.actor, "the event actor")?,
                    object: cursor.require_value(frame.object, "the sacrificed object query")?,
                    exclude_source: frame.exclude_source,
                }));
            }
        };
    }
}
