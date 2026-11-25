//! The one modifier tail of an event clause (S5, docs/oracle-event-model.md).
//!
//! ```text
//! ModifierTail := ( "and" ("isn't" | "isn’t") "blocked"
//!                 | "for the first time each turn"
//!                 | "during" TurnOwner
//!                 | "during combat" )*
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
///
/// The first four slots are read by the shared tail in any event frame;
/// the event-slot qualifiers (`without_dying`, `excluded_origin`) are read
/// by [`read_event_qualifiers`] only after a frame that types them, so a
/// subject kind that does not know a slot can never receive it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventModifierTail {
    /// `and isn't blocked` (CR 509.1h): the attacker became unblocked.
    pub not_blocked: bool,
    /// `for the first time each turn`: the subject's per-turn ordinal is 1.
    pub first_time_each_turn: bool,
    /// `during your turn` / `during an opponent's turn` (CR 500.1).
    pub turn_owner: Option<EventTurnOwner>,
    /// `during combat` (CR 506.1): the event happens during the combat
    /// phase. Only a zone-change pattern that records the step evaluates it.
    pub during_combat: bool,
    /// `leave(s) the battlefield without dying` (CR 700.4): the departure
    /// is not a death, a matcher field of the same departure pattern.
    pub without_dying: bool,
    /// `put into a graveyard from anywhere other than ZONE`: the origin the
    /// arrival must not come from (CR 400.3), a field of the same arrival.
    pub excluded_origin: Option<&'static str>,
}

impl EventModifierTail {
    pub(in crate::oracle::canonical) fn is_empty(&self) -> bool {
        !self.not_blocked
            && !self.first_time_each_turn
            && self.turn_owner.is_none()
            && !self.during_combat
            && !self.without_dying
            && self.excluded_origin.is_none()
    }

    /// The written name of the first modifier, for engine-gap diagnostics.
    pub(in crate::oracle::canonical) fn first_modifier_name(&self) -> Option<&'static str> {
        if self.not_blocked {
            Some("`and isn't blocked`")
        } else if self.first_time_each_turn {
            Some("`for the first time each turn`")
        } else if self.during_combat {
            Some("`during combat`")
        } else if self.without_dying {
            Some("`without dying`")
        } else if self.excluded_origin.is_some() {
            Some("`from anywhere other than`")
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

/// The written length of a turn owner after `during` at `offset`, or
/// `None`: `your turn`, `an opponent's turn`, and the distributive `each of
/// your turns` / `each opponent's turn` (CR 500.1). A lexical probe.
fn turn_owner_phrase_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(usize, bool)> {
    if word_at(cursor, offset, "your") && word_at(cursor, offset + 1, "turn") {
        Some((2, false))
    } else if word_at(cursor, offset, "an")
        && opponent_possessive_at(cursor, offset + 1)
        && word_at(cursor, offset + 2, "turn")
    {
        Some((3, false))
    } else if word_at(cursor, offset, "each")
        && opponent_possessive_at(cursor, offset + 1)
        && word_at(cursor, offset + 2, "turn")
    {
        Some((3, true))
    } else if word_at(cursor, offset, "each")
        && word_at(cursor, offset + 1, "of")
        && word_at(cursor, offset + 2, "your")
        && word_at(cursor, offset + 3, "turns")
    {
        Some((4, true))
    } else {
        None
    }
}

/// `during TURN-OWNER` starts at the cursor (`during your turn`, `during
/// an opponent's turn`, `during each opponent's turn`, `during each of your
/// turns`). A lexical probe that commits nothing.
pub(in crate::oracle::canonical) fn turn_owner_modifier_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "during") && turn_owner_phrase_at(cursor, 1).is_some()
}

/// Reads a turn owner after `during` (CR 500.1). The distributive forms
/// (`each opponent's turn`, `each of your turns`) name the same relation for
/// every turn; the returned flag says one was written, so an ordinal parent
/// (`your first spell during each opponent's turn`) knows its counting
/// period is the turn.
pub(in crate::oracle::canonical) fn recognize_turn_owner_period(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(EventTurnOwner, bool)> {
    cursor.enter("event.modifier.turn-owner");
    cursor.consume("during")?;
    let (length, distributive) = turn_owner_phrase_at(cursor, 0).ok_or_else(|| {
        cursor.fail(
            "`your turn`, `an opponent's turn`, `each opponent's turn` or `each of your turns`",
        )
    })?;
    let owner = if word_at(cursor, 0, "your") || word_at(cursor, 2, "your") {
        EventTurnOwner::ControllerTurn
    } else {
        EventTurnOwner::OpponentTurn
    };
    cursor.advance_to(cursor.position() + length)?;
    Ok((owner, distributive))
}

/// Reads `during your turn` or `during an opponent's turn` (CR 500.1), or
/// one of their distributive forms.
pub(in crate::oracle::canonical) fn recognize_turn_owner_modifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EventTurnOwner> {
    recognize_turn_owner_period(cursor).map(|(owner, _)| owner)
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

fn during_combat_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "during") && word_at(cursor, 1, "combat")
}

fn duplicate(cursor: &OracleTokenCursor<'_>) -> MachineFailure {
    cursor.fail("each event modifier at most once")
}

/// The zone an entering object comes from (`enters from your graveyard`).
/// The engine records the origin zone on the entering object; `your` names
/// the owner's zone, which only the source itself is known to have left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EntryOriginNode {
    pub zone: &'static str,
    /// Written `your ZONE` (relative to the ability controller).
    pub own: bool,
}

/// `from` starts an entry origin at the cursor (a lexical probe).
pub(in crate::oracle::canonical) fn entry_origin_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "from")
}

/// Reads `from (your|a) graveyard | from your hand | from your library |
/// from exile` after `enters [the battlefield]`.
pub(in crate::oracle::canonical) fn recognize_entry_origin(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EntryOriginNode> {
    cursor.enter("event.modifier.entry-origin");
    cursor.consume("from")?;
    if word_at(cursor, 0, "exile") {
        cursor.consume("exile")?;
        return Ok(EntryOriginNode {
            zone: "exile",
            own: false,
        });
    }
    let own = word_at(cursor, 0, "your");
    if own {
        cursor.consume("your")?;
    } else {
        cursor.consume("a")?;
    }
    cursor.enter("event.modifier.entry-origin.zone");
    let zone = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("graveyard") => "graveyard",
        Some("hand") if own => "hand",
        Some("library") if own => "library",
        _ => return Err(cursor.fail("a graveyard, your hand, your library or exile")),
    };
    cursor.consume(zone)?;
    Ok(EntryOriginNode { zone, own })
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

fn without_dying_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "without") && word_at(cursor, 1, "dying")
}

/// `other than ZONE` right after a written `from anywhere` (the arrival
/// frame reads `from anywhere`; the exclusion is this qualifier's).
fn excluded_origin_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let position = cursor.position();
    let previous = cursor.token_slice(position.saturating_sub(2), position);
    previous.len() == 2
        && previous[0].raw.eq_ignore_ascii_case("from")
        && previous[1].raw.eq_ignore_ascii_case("anywhere")
        && word_at(cursor, 0, "other")
        && word_at(cursor, 1, "than")
}

/// `during PERIOD` that the shared tail does not type (`during the declare
/// attackers step`, `during your untap step`): a turn window of the shared
/// window grammar. A lexical probe.
fn step_window_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "during")
        && !turn_owner_modifier_starts(cursor)
        && !during_combat_starts(cursor)
}

/// Reads `other than ZONE` after `from anywhere` (CR 400.3): the zone an
/// arriving card must not come from.
fn recognize_excluded_origin(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<&'static str> {
    cursor.enter("event.qualifier.excluded-origin");
    cursor.consume_phrase(&["other", "than"])?;
    cursor.enter("event.qualifier.excluded-origin.zone");
    if word_at(cursor, 0, "the") && word_at(cursor, 1, "battlefield") {
        cursor.consume_phrase(&["the", "battlefield"])?;
        return Ok("battlefield");
    }
    if word_at(cursor, 0, "exile") {
        cursor.consume("exile")?;
        return Ok("exile");
    }
    cursor.consume("your")?;
    let zone = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("hand") => "hand",
        Some("library") => "library",
        _ => return Err(cursor.fail("the battlefield, exile, your hand or your library")),
    };
    cursor.consume(zone)?;
    Ok(zone)
}

/// The event-slot qualifiers after a complete occurrence frame (S5): each
/// is an independent typed slot of the event pattern, filled only on a
/// frame that types it and lowered to the matcher field of that same
/// pattern. A qualifier on any other frame fails explicitly; a qualifier
/// that no matcher evaluates is an engine gap, never dropped.
///
/// ```text
/// Qualifier := "without" "dying"                    -- a departure (CR 700.4)
///            | "other" "than" ZONE                  -- after `from anywhere`
///            | TurnWindow                           -- `during the STEP`
/// ```
///
/// The legacy tail (`read_event_modifier_tail`) keeps the turn owner, the
/// turn ordinal, `during combat` and `and isn't blocked`; the enclosing
/// clause reads it after these qualifiers.
pub(in crate::oracle::canonical) fn read_event_qualifiers(
    cursor: &mut OracleTokenCursor<'_>,
    mut event: super::GenericTriggerEventNode,
) -> MachineResult<super::GenericTriggerEventNode> {
    loop {
        if without_dying_starts(cursor) {
            cursor.enter("event.qualifier.without-dying");
            cursor.consume_phrase(&["without", "dying"])?;
            let super::GenericTriggerEventNode::Object(node) = &mut event else {
                return Err(
                    cursor.fail("`without dying` after a permanent leaving the battlefield")
                );
            };
            cursor.require(
                node.verb == super::ObjectEventVerb::LeaveBattlefield,
                "`without dying` after a permanent leaving the battlefield",
            )?;
            if node.modifiers.without_dying {
                return Err(duplicate(cursor));
            }
            node.modifiers.without_dying = true;
        } else if excluded_origin_starts(cursor) {
            let zone = recognize_excluded_origin(cursor)?;
            cursor.enter("event.qualifier.excluded-origin.frame");
            let super::GenericTriggerEventNode::Object(node) = &mut event else {
                return Err(cursor.fail("an origin exclusion after a card put into a graveyard"));
            };
            cursor.require(
                matches!(node.verb, super::ObjectEventVerb::PutIntoGraveyard(_, None)),
                "an origin exclusion after a card put into a graveyard from anywhere",
            )?;
            if node.modifiers.excluded_origin.is_some() {
                return Err(duplicate(cursor));
            }
            node.modifiers.excluded_origin = Some(zone);
        } else if step_window_starts(cursor) {
            let window = super::recognize_turn_window(cursor, super::WindowSubject::AbilityPlayer)?;
            let period = window
                .steps
                .steps()
                .map(super::WindowStep::engine_name)
                .collect::<Vec<_>>()
                .join(" or ");
            // The typed window is read; no event pattern records the step it
            // happened in beyond `during combat`.
            cursor.enter("event.qualifier.step-window");
            return Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::EventMatcher,
                &format!("an engine event matcher for an event during the {period} step"),
            ));
        } else {
            return Ok(event);
        }
    }
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
        } else if during_combat_starts(cursor) {
            cursor.enter("event.modifier.during-combat");
            if tail.during_combat {
                return Err(duplicate(cursor));
            }
            cursor.consume_phrase(&["during", "combat"])?;
            tail.during_combat = true;
        } else {
            return Ok(tail);
        }
    }
}

#[cfg(test)]
#[path = "event_modifiers_tests.rs"]
mod tests;
