//! Replaced events read after `would` (CR 614.1a, 614.10, 121.6).
//!
//! ```text
//! Replacement := "If" PLAYER "would" EVENT "," PROGRAM "instead" "."
//! EVENT       := "draw" "a" "card" [DRAW-EXCEPTION]
//! DRAW-EXCEPTION := "except" "the" "first" "one" PLAYER "draw" "in" "each"
//!                   "of" PLAYER's "draw" "steps"
//! PROGRAM     := "draw" N "cards"
//! ```
//!
//! The subject and the event verb are read with the shared clause lexicon
//! (`consume_lemma`); the frame commits once both are read, so another
//! replacement family keeps an event this reader does not own. A replacement
//! program the runtime cannot apply to the replaced event fails at its own
//! state instead of being approximated.
use super::super::{ChildOutcome, CommitPoint, MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, NumericAtom, VerbAgreement, VerbLemma, consume_lemma,
    recognize_numeric_atom,
};

/// The replaced event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReplacedEventNode {
    /// `PLAYER would draw a card` (CR 121.6): one draw, possibly excepting
    /// the first draw of the player's draw step.
    Draw {
        player: LocalPlayerReference,
        except_first_in_draw_step: bool,
    },
}

/// The program performed instead of the replaced event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReplacementProgramNode {
    /// `draw N cards`: the replaced draw becomes N draws.
    Draw { count: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EventReplacementNode {
    pub event: ReplacedEventNode,
    pub program: ReplacementProgramNode,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// `except the first one you draw in each of your draw steps` (CR 504.1).
fn recognize_first_draw_step_exception(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("replacement.event.draw.exception");
    cursor.consume_phrase(&["except", "the", "first", "one", "you"])?;
    consume_lemma(cursor, VerbLemma::Draw, VerbAgreement::Base)?;
    cursor.enter("replacement.event.draw.exception.step");
    cursor.consume_phrase(&["in", "each", "of", "your", "draw", "steps"])?;
    Ok(())
}

fn recognize_draw_event(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ReplacedEventNode> {
    cursor.enter("replacement.event.draw.object");
    cursor.consume_phrase(&["a", "card"])?;
    let except_first_in_draw_step = if word_at(cursor, 0, "except") {
        recognize_first_draw_step_exception(cursor)?;
        true
    } else {
        false
    };
    Ok(ReplacedEventNode::Draw {
        player: LocalPlayerReference::Controller,
        except_first_in_draw_step,
    })
}

/// `draw N cards instead` for a replaced draw.
fn recognize_draw_program(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ReplacementProgramNode> {
    cursor.enter("replacement.program.draw");
    consume_lemma(cursor, VerbLemma::Draw, VerbAgreement::Base)?;
    cursor.enter("replacement.program.draw.count");
    let count = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(count) if count >= 2 => count,
        _ => {
            return Err(cursor.fail("a literal number of cards greater than the one replaced draw"));
        }
    };
    cursor.consume("cards")?;
    Ok(ReplacementProgramNode::Draw { count })
}

fn recognize_event_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    commit: &mut CommitPoint,
) -> MachineResult<EventReplacementNode> {
    cursor.enter("replacement.event.introducer");
    cursor.consume("if")?;
    cursor.enter("replacement.event.subject");
    // The ability controller: the runtime replaces only its own draws.
    cursor.consume("you")?;
    cursor.consume("would")?;
    cursor.enter("replacement.event.verb");
    consume_lemma(cursor, VerbLemma::Draw, VerbAgreement::Base)?;
    commit.commit();
    let event = recognize_draw_event(cursor)?;
    cursor.enter("replacement.event.separator");
    cursor.consume(",")?;
    let program = match &event {
        ReplacedEventNode::Draw { .. } => recognize_draw_program(cursor)?,
    };
    cursor.enter("replacement.event.instead");
    cursor.consume_phrase(&["instead", "."])?;
    cursor.enter("replacement.event.complete");
    cursor.finish()?;
    Ok(EventReplacementNode { event, program })
}

/// Reads one complete event replacement; `NoMatch` before its subject and
/// event verb are read.
pub(in crate::oracle::canonical) fn try_recognize_event_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> ChildOutcome<EventReplacementNode> {
    cursor.transaction(recognize_event_replacement)
}

#[cfg(test)]
#[path = "replacement_event_tests.rs"]
mod tests;
