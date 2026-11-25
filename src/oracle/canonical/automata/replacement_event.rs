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
use super::*;
use super::{
    LocalPlayerReference, NumericAtom, VerbAgreement, VerbLemma, consume_lemma,
    recognize_numeric_atom,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// The replaced event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReplacedEventNode {
    /// `PLAYER would draw a card` (CR 121.6): one draw, possibly excepting
    /// the first draw of the player's draw step.
    Draw {
        player: LocalPlayerReference,
        except_first_in_draw_step: bool,
    },
    /// `PLAYER would lose unspent mana` (CR 500.4): the player's mana pool
    /// empties as a step or phase ends.
    LoseUnspentMana { player: LocalPlayerReference },
}

/// The program performed instead of the replaced event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReplacementProgramNode {
    /// `draw N cards`: the replaced draw becomes N draws.
    Draw { count: i64 },
    /// `that mana becomes COLOR`: the mana stays in the pool as that type
    /// (`W`, `U`, `B`, `R`, `G` or `C`, CR 106.4).
    ManaBecomes { symbol: &'static str },
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
    // The program owns the diagnosis of its count: an article (`draw a card
    // instead`) is an atom the numeric child may reject at its own state.
    let mut amount = cursor.clone();
    let count = match recognize_numeric_atom(&mut amount) {
        Ok(NumericAtom::Integer(count)) if count >= 2 => {
            *cursor = amount;
            count
        }
        _ => {
            return Err(cursor.fail("a literal number of cards greater than the one replaced draw"));
        }
    };
    cursor.consume("cards")?;
    Ok(ReplacementProgramNode::Draw { count })
}

/// `that mana becomes COLOR instead`: the unspent mana stays as one type.
fn recognize_mana_type_program(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ReplacementProgramNode> {
    cursor.enter("replacement.program.mana-type");
    cursor.consume_phrase(&["that", "mana", "becomes"])?;
    cursor.enter("replacement.program.mana-type.color");
    let symbol = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("white") => "W",
        Some("blue") => "U",
        Some("black") => "B",
        Some("red") => "R",
        Some("green") => "G",
        Some("colorless") => "C",
        _ => return Err(cursor.fail("a color or `colorless` naming the retained mana type")),
    };
    let raw = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&raw)?;
    Ok(ReplacementProgramNode::ManaBecomes { symbol })
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
    let event = if word_at(cursor, 0, "lose")
        && word_at(cursor, 1, "unspent")
        && word_at(cursor, 2, "mana")
    {
        consume_lemma(cursor, VerbLemma::Lose, VerbAgreement::Base)?;
        commit.commit();
        cursor.enter("replacement.event.unspent-mana");
        cursor.consume_phrase(&["unspent", "mana"])?;
        ReplacedEventNode::LoseUnspentMana {
            player: LocalPlayerReference::Controller,
        }
    } else {
        consume_lemma(cursor, VerbLemma::Draw, VerbAgreement::Base)?;
        commit.commit();
        recognize_draw_event(cursor)?
    };
    cursor.enter("replacement.event.separator");
    cursor.consume(",")?;
    let program = match &event {
        ReplacedEventNode::Draw { .. } => recognize_draw_program(cursor)?,
        ReplacedEventNode::LoseUnspentMana { .. } => recognize_mana_type_program(cursor)?,
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

/// A replacement effect one sentence of a resolving program creates
/// (CR 611.2a, 614.1a): the shared departure frame, or a quantity
/// replacement after a preposed `Until end of turn,`.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum CreatedReplacementRuleNode {
    Departure(DepartureReplacementNode),
    Quantity {
        replacement: QuantityReplacementNode,
        lifetime: Option<CreatedRuleLifetime>,
    },
    /// A damage replacement or prevention through this turn
    /// (`damage_replacement.rs`).
    Damage(DamageReplacementNode),
}

impl CreatedReplacementRuleNode {
    /// `dealt damage this way`: the replacement follows the objects an
    /// earlier damage action of the program dealt damage to.
    pub(in crate::oracle::canonical) fn follows_damaged_objects(&self) -> bool {
        matches!(
            self,
            Self::Departure(DepartureReplacementNode {
                subject: DepartureSubjectNode::Objects(objects),
                ..
            }) if objects.damaged_this_way
        )
    }
}

/// Lexical lookahead: a sentence that may open a created replacement.
pub(in crate::oracle::canonical) fn created_replacement_sentence_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    departure_replacement_starts(cursor)
        || created_damage_replacement_starts(cursor)
        || quantity_replacement_sentence_starts(cursor)
}

/// `[Until end of turn,] if ... would (be created|be put|create|put) ...`
/// before the sentence's first comma after `if`.
fn quantity_replacement_sentence_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let offset = preposed_end_of_turn_width(cursor);
    if !word_at(cursor, offset, "if") {
        return false;
    }
    (offset + 1..)
        .take_while(|index| {
            cursor
                .peek_ahead(*index)
                .is_some_and(|raw| raw != "," && raw != ".")
        })
        .any(|index| {
            word_at(cursor, index, "would")
                && (word_at(cursor, index + 1, "create")
                    || word_at(cursor, index + 1, "put")
                    || (word_at(cursor, index + 1, "be")
                        && (word_at(cursor, index + 2, "created")
                            || word_at(cursor, index + 2, "put"))))
        })
}

fn preposed_end_of_turn_width(cursor: &OracleTokenCursor<'_>) -> usize {
    super::embedded_delayed_trigger::preposed_lifetime_width(cursor).unwrap_or(0)
}

/// End (exclusive, absolute) of the sentence starting at the cursor.
fn sentence_end(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (0..)
        .take_while(|offset| cursor.peek_ahead(*offset).is_some())
        .find(|offset| cursor.peek_ahead(*offset) == Some("."))
        .map(|offset| cursor.position() + offset + 1)
}

/// Reads one sentence creating a replacement, through its period; `NoMatch`
/// until the departure frame or the quantity event commits.
pub(in crate::oracle::canonical) fn try_recognize_created_replacement_sentence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> ChildOutcome<CreatedReplacementRuleNode> {
    match try_recognize_departure_replacement(
        cursor,
        DepartureFrame::Sentence,
        name_links,
        name_failure,
    ) {
        ChildOutcome::Matched(node) => {
            return ChildOutcome::Matched(CreatedReplacementRuleNode::Departure(node));
        }
        ChildOutcome::Committed(failure) => return ChildOutcome::Committed(failure),
        ChildOutcome::NoMatch(_) => {}
    }
    let context = DamageReplacementContext {
        name_links,
        name_failure,
        attachment_word: None,
    };
    match try_recognize_created_damage_replacement(cursor, &context) {
        ChildOutcome::Matched(node) => {
            return ChildOutcome::Matched(CreatedReplacementRuleNode::Damage(node));
        }
        ChildOutcome::Committed(failure) => return ChildOutcome::Committed(failure),
        ChildOutcome::NoMatch(_) => {}
    }
    let checkpoint = cursor.checkpoint();
    cursor.enter("replacement.created-quantity.lifetime");
    let lifetime = if preposed_end_of_turn_width(cursor) > 0 {
        let lifetime = super::embedded_delayed_trigger::recognize_created_rule_lifetime(cursor)
            .and_then(|lifetime| cursor.consume(",").map(|_| lifetime));
        match lifetime {
            Ok(lifetime) => Some(lifetime),
            Err(failure) => {
                cursor.rollback(checkpoint);
                return ChildOutcome::NoMatch(failure);
            }
        }
    } else {
        None
    };
    let Some(end) = sentence_end(cursor) else {
        let failure = cursor.fail("a sentence closed by a period");
        cursor.rollback(checkpoint);
        return ChildOutcome::NoMatch(failure);
    };
    let mut claimed = true;
    let result = cursor.bounded(end, |cursor| {
        match try_recognize_quantity_replacement(cursor) {
            QuantityReplacementRecognition::Parsed(node) => Ok(node),
            QuantityReplacementRecognition::Failed(failure) => Err(failure),
            QuantityReplacementRecognition::NotClaimed => {
                claimed = false;
                Err(cursor.fail("a quantity replacement event"))
            }
        }
    });
    match result {
        Ok(replacement) => ChildOutcome::Matched(CreatedReplacementRuleNode::Quantity {
            replacement,
            lifetime,
        }),
        Err(failure) => {
            cursor.rollback(checkpoint);
            if claimed {
                ChildOutcome::Committed(failure)
            } else {
                ChildOutcome::NoMatch(failure)
            }
        }
    }
}
