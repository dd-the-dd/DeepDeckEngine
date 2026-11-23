//! The one modifier tail of an event clause (S5, docs/oracle-event-model.md).
//!
//! ```text
//! ModifierTail := ( "and" ("isn't" | "isn’t") "blocked"
//!                 | "for the first time each turn"
//!                 | "during" TurnOwner )*
//! TurnOwner    := "your turn" | "an opponent's turn"
//! Alone        := "alone"            -- read by the attack verb it follows
//! ```
//!
//! Each modifier is a typed attribute of the event pattern, read once for
//! every subject kind. Recognizing a modifier is not runtime support: the
//! lowering of each event kind decides which modifiers its matcher evaluates,
//! and an unsupported one fails as an engine gap (`engine.gap.event-matcher`),
//! never dropped.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};

/// Whose turn an event happens in, relative to the ability controller
/// (CR 500.1), whoever performs the event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EventTurnOwner {
    /// `during your turn`.
    ControllerTurn,
    /// `during an opponent's turn`.
    OpponentTurn,
}

/// The clause-level modifiers after an event verb and its complement, in
/// one typed record. Each modifier occurs at most once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventModifierTail {
    /// `and isn't blocked` (CR 509.1h): the attacker became unblocked.
    pub not_blocked: bool,
    /// `for the first time each turn`: the subject's per-turn ordinal is 1.
    pub first_time_each_turn: bool,
    /// `during your turn` / `during an opponent's turn` (CR 500.1).
    pub turn_owner: Option<EventTurnOwner>,
}

impl EventModifierTail {
    pub(in crate::oracle::canonical) fn is_empty(&self) -> bool {
        !self.not_blocked && !self.first_time_each_turn && self.turn_owner.is_none()
    }

    /// The written name of the first modifier, for engine-gap diagnostics.
    pub(in crate::oracle::canonical) fn first_modifier_name(&self) -> Option<&'static str> {
        if self.not_blocked {
            Some("`and isn't blocked`")
        } else if self.first_time_each_turn {
            Some("`for the first time each turn`")
        } else {
            self.turn_owner.map(|owner| match owner {
                EventTurnOwner::ControllerTurn => "`during your turn`",
                EventTurnOwner::OpponentTurn => "`during an opponent's turn`",
            })
        }
    }
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn opponent_possessive_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    word_at(cursor, offset, "opponent's") || word_at(cursor, offset, "opponent’s")
}

/// `during your turn` or `during an opponent's turn` starts at the cursor.
/// A lexical probe that commits nothing.
pub(in crate::oracle::canonical) fn turn_owner_modifier_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "during")
        && ((word_at(cursor, 1, "your") && word_at(cursor, 2, "turn"))
            || (word_at(cursor, 1, "an")
                && opponent_possessive_at(cursor, 2)
                && word_at(cursor, 3, "turn")))
}

/// Reads `during your turn` or `during an opponent's turn` (CR 500.1).
pub(in crate::oracle::canonical) fn recognize_turn_owner_modifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EventTurnOwner> {
    cursor.enter("event.modifier.turn-owner");
    cursor.consume("during")?;
    if word_at(cursor, 0, "your") {
        cursor.consume_phrase(&["your", "turn"])?;
        return Ok(EventTurnOwner::ControllerTurn);
    }
    cursor.consume("an")?;
    let possessive = cursor
        .peek()
        .filter(|_| opponent_possessive_at(cursor, 0))
        .ok_or_else(|| cursor.fail("`your turn` or `an opponent's turn`"))?
        .to_string();
    cursor.consume(&possessive)?;
    cursor.consume("turn")?;
    Ok(EventTurnOwner::OpponentTurn)
}

/// Reads `alone` after an attack verb (CR 506.5). Returns whether it was
/// written; the caller decides that only an attack takes it.
pub(in crate::oracle::canonical) fn consume_alone_modifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    if !word_at(cursor, 0, "alone") {
        return Ok(false);
    }
    cursor.enter("event.modifier.alone");
    cursor.consume("alone")?;
    Ok(true)
}

fn not_blocked_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "and")
        && (word_at(cursor, 1, "isn't") || word_at(cursor, 1, "isn’t"))
        && word_at(cursor, 2, "blocked")
}

fn first_time_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "for")
        && word_at(cursor, 1, "the")
        && word_at(cursor, 2, "first")
        && word_at(cursor, 3, "time")
}

fn duplicate(cursor: &OracleTokenCursor<'_>) -> MachineFailure {
    cursor.fail("each event modifier at most once")
}

/// Reads the modifier tail in written order. Tokens that start no modifier
/// are left to the enclosing clause (its boundary or a body).
pub(in crate::oracle::canonical) fn read_event_modifier_tail(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EventModifierTail> {
    let mut tail = EventModifierTail::default();
    loop {
        if not_blocked_starts(cursor) {
            cursor.enter("event.modifier.not-blocked");
            if tail.not_blocked {
                return Err(duplicate(cursor));
            }
            cursor.advance_to(cursor.position() + 3)?;
            tail.not_blocked = true;
        } else if first_time_starts(cursor) {
            cursor.enter("event.modifier.turn-ordinal");
            if tail.first_time_each_turn {
                return Err(duplicate(cursor));
            }
            cursor.consume_phrase(&["for", "the", "first", "time", "each", "turn"])?;
            tail.first_time_each_turn = true;
        } else if turn_owner_modifier_starts(cursor) {
            if tail.turn_owner.is_some() {
                cursor.enter("event.modifier.turn-owner");
                return Err(duplicate(cursor));
            }
            tail.turn_owner = Some(recognize_turn_owner_modifier(cursor)?);
        } else {
            return Ok(tail);
        }
    }
}

#[cfg(test)]
#[path = "event_modifiers_tests.rs"]
mod tests;
