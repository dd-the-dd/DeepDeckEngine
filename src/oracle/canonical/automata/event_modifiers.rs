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
//!
//! ```text
//! EventTimeCondition := "while" CONDITION   -- the one condition reader
//! ```
//!
//! `while CONDITION` and a turn owner that no pattern matcher evaluates are
//! the event-time condition of the occurrence: a state checked only as the
//! event occurs (CR 603.2), distinct from the intervening `if` that is checked
//! again on resolution (CR 603.4) and from conditions of the effect program.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

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

/// The state an occurrence requires when it happens (CR 603.2): the turn
/// owner of an event kind without a pattern-level turn matcher, and a written
/// `while CONDITION`. It is evaluated once, as the event occurs; it is never
/// rechecked on resolution (that is the intervening `if`, CR 603.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventTimeConditionNode {
    pub turn_owner: Option<EventTurnOwner>,
    pub condition: Option<super::SentenceConditionNode>,
}

impl EventTimeConditionNode {
    pub(in crate::oracle::canonical) fn is_empty(&self) -> bool {
        self.turn_owner.is_none() && self.condition.is_none()
    }
}

/// `while` starts an event-time condition at the cursor (a lexical probe).
pub(in crate::oracle::canonical) fn while_condition_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "while")
}

/// Reads `while CONDITION` after a complete occurrence. The condition is the
/// one condition language (`recognize_condition_atom`) in the state scope: no
/// cast record, previous action or target exists when the event occurs. The
/// ability's own source is the only object referent. The enclosing clause
/// owns the following boundary.
pub(in crate::oracle::canonical) fn recognize_while_condition(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<super::SentenceConditionNode> {
    cursor.enter("event.modifier.while");
    cursor.consume("while")?;
    let effects =
        super::EffectRecognitionContext::for_source(super::LocalObjectReference::SourceSelf);
    let program = super::SentenceConditionContext {
        name_links,
        name_failure,
        effects: &effects,
        card_target: None,
        card_focus: false,
        departed_targets: &[],
    };
    let context = super::ConditionContext::program(super::ConditionScope::Static, &program);
    cursor.enter("event.modifier.while.condition");
    super::recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        super::recognize_condition_atom(cursor, &context)
    })
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
