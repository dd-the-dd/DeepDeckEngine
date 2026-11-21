use super::super::{MachineResult, OracleTokenCursor};
use crate::oracle::characteristic_vocabulary::{PlayerRole, lookup_player_role};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EventActorScope {
    Controller,
    AnyPlayer,
    Opponent,
}

#[derive(Clone, Copy)]
enum EventActorState {
    Subject,
    PlayerRole,
    Complete,
}

/// Shared subject reader. Event parents own the verb and object domain.
/// A non-player subject is a transactional mismatch, not an event claim.
pub(in crate::oracle::canonical) fn try_recognize_event_actor_scope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<EventActorScope>> {
    let checkpoint = cursor.checkpoint();
    let mut state = EventActorState::Subject;
    let mut article = None;
    let mut scope = None;
    loop {
        state = match state {
            EventActorState::Subject => {
                cursor.enter("event.actor.subject");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
                {
                    cursor.consume("you")?;
                    scope = Some(EventActorScope::Controller);
                    EventActorState::Complete
                } else if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                }) {
                    let raw = cursor.peek().expect("checked article").to_string();
                    cursor.consume(&raw)?;
                    article = Some(raw);
                    EventActorState::PlayerRole
                } else {
                    cursor.rollback(checkpoint);
                    return Ok(None);
                }
            }
            EventActorState::PlayerRole => {
                cursor.enter("event.actor.player-role");
                let Some(role) = cursor.peek().and_then(lookup_player_role) else {
                    cursor.rollback(checkpoint);
                    return Ok(None);
                };
                cursor.require(
                    matches!(
                        (
                            article.as_deref().map(str::to_ascii_lowercase).as_deref(),
                            role
                        ),
                        (Some("a"), PlayerRole::Any) | (Some("an"), PlayerRole::Opponent)
                    ),
                    "the normalized article for this player role",
                )?;
                cursor.advance_to(cursor.position() + 1)?;
                scope = Some(match role {
                    PlayerRole::Any => EventActorScope::AnyPlayer,
                    PlayerRole::Opponent => EventActorScope::Opponent,
                });
                EventActorState::Complete
            }
            EventActorState::Complete => {
                cursor.enter("event.actor.complete");
                return cursor.require_value(scope, "a typed event actor").map(Some);
            }
        };
    }
}

/// `one or more` PLURAL-ROLE (`opponents`, `players`): the plural actor of a
/// per-event player trigger. A non-matching prefix is a transactional miss.
pub(in crate::oracle::canonical) fn try_recognize_plural_event_actor_scope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<EventActorScope>> {
    let checkpoint = cursor.checkpoint();
    cursor.enter("event.actor.plural-quantifier");
    if cursor
        .peek()
        .is_none_or(|raw| !raw.eq_ignore_ascii_case("one"))
        || cursor.consume_phrase(&["one", "or", "more"]).is_err()
    {
        cursor.rollback(checkpoint);
        return Ok(None);
    }
    cursor.enter("event.actor.plural-role");
    let scope = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("opponents") => EventActorScope::Opponent,
        Some("players") => EventActorScope::AnyPlayer,
        _ => {
            cursor.rollback(checkpoint);
            return Ok(None);
        }
    };
    cursor.advance_to(cursor.position() + 1)?;
    Ok(Some(scope))
}

#[cfg(test)]
#[path = "event_actor_tests.rs"]
mod tests;
