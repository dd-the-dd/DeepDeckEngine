//! Program control over bound sets and repeated processes (CR 608.2c).
//!
//! ```text
//! PlayerIteration := "for" "each" Population ","            BODY "."
//! Population      := "player" | "opponent" | "other" "player"
//! MemberIteration := "for" "each" "of" SetAnaphor ","       BODY "."
//! SetAnaphor      := "them" | "those" NOUN-PHRASE
//! RepeatProcess   := "repeat" "this" "process" [Bound]
//!                  | "repeat" "the" "following" "process" Amount "times"
//! Bound           := "once" | Amount "more" "times"
//! ```
//!
//! The introducers are structural: the sequence owns the sentence whose
//! actions form the body, and the body's references (`that player`,
//! `that opponent`, `they`, `that creature`, `it`) name the member being
//! iterated. Each frame is a typed range of the program; the lowering wraps
//! the range into the shared `forEachPlayer` / `forEachObject` loops. A
//! repeated process is an ordinary instruction of the program whose
//! performance asks the runtime to perform the process again; the process
//! itself is the program read before it, never guessed from the text.
use super::super::{MachineResult, OracleTokenCursor};
use super::{NumericExpression, TargetBinding};

/// The population a `For each PLAYER,` frame iterates in turn order
/// (CR 101.4, 102.2, 102.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum IterationPopulation {
    /// `each player`: every player still in the game.
    Players,
    /// `each opponent`: the opponents of the ability's controller.
    Opponents,
    /// `each other player`: every player but the ability's controller.
    OtherPlayers,
}

/// What one iteration frame ranges over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum IterationDomain {
    /// `For each opponent, ...`: the body names the member as `that
    /// player`, `that opponent` or `they`.
    Players(IterationPopulation),
    /// `For each of them, ...` / `For each of those creatures, ...`: the
    /// objects one plural target slot of the program declared (CR 115.1).
    /// The body names the member as `that creature` or `it`.
    DeclaredObjects(TargetBinding),
}

/// One distributive frame over the actions `first_effect..end_effect` of a
/// program: the body sentence is performed once per member, in turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct IterationFrameNode {
    pub first_effect: usize,
    pub end_effect: usize,
    pub domain: IterationDomain,
}

impl IterationFrameNode {
    /// The same frame over a sequence whose actions are shifted by `offset`.
    pub(in crate::oracle::canonical) fn shifted(self, offset: usize) -> Self {
        let domain = match self.domain {
            IterationDomain::DeclaredObjects(binding) => {
                IterationDomain::DeclaredObjects(TargetBinding {
                    effect_index: binding.effect_index + offset,
                    ..binding
                })
            }
            players => players,
        };
        Self {
            first_effect: self.first_effect + offset,
            end_effect: self.end_effect + offset,
            domain,
        }
    }
}

/// Which process a repetition names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RepeatedProcess {
    /// `repeat this process`: the program read before the instruction,
    /// performed again while this instruction is reached. `additional`
    /// bounds the extra performances (`once`, `X more times`); without a
    /// bound only the program's own conditions end the repetition.
    Preceding {
        additional: Option<NumericExpression>,
    },
    /// `repeat the following process N times`: the rest of the program,
    /// performed `times` times in a row.
    Following { times: NumericExpression },
}

/// The instruction `repeat this process` (CR 608.2c): a step of the
/// program, so a conditional or an outcome connector may govern it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RepeatProcessNode {
    pub process: RepeatedProcess,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The population and token width of `for each opponent|player|other
/// player ,` at the cursor. Lexical lookahead only.
pub(in crate::oracle::canonical) fn player_iteration_population_at(
    cursor: &OracleTokenCursor<'_>,
) -> Option<(IterationPopulation, usize)> {
    if !(word_at(cursor, 0, "for") && word_at(cursor, 1, "each")) {
        return None;
    }
    let (population, width) = if word_at(cursor, 2, "opponent") {
        (IterationPopulation::Opponents, 3)
    } else if word_at(cursor, 2, "player") {
        (IterationPopulation::Players, 3)
    } else if word_at(cursor, 2, "other") && word_at(cursor, 3, "player") {
        (IterationPopulation::OtherPlayers, 4)
    } else {
        return None;
    };
    word_at(cursor, width, ",").then_some((population, width + 1))
}

/// Reads `for each opponent|player|other player ,` and returns the
/// population; the parent owns the body sentence that follows.
pub(in crate::oracle::canonical) fn recognize_player_iteration_introducer(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<IterationPopulation> {
    cursor.enter("sequence.for-each-player.population");
    let (population, width) = player_iteration_population_at(cursor)
        .ok_or_else(|| cursor.fail("`for each player|opponent|other player ,`"))?;
    let end = cursor.position() + width - 1;
    cursor.advance_to(end)?;
    cursor.enter("sequence.for-each-player.separator");
    cursor.consume(",")?;
    Ok(population)
}

/// `for each of them|those ... ,` at the cursor: the partitive determiner
/// over a set the program bound. Lexical lookahead only.
pub(in crate::oracle::canonical) fn member_iteration_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "for")
        && word_at(cursor, 1, "each")
        && word_at(cursor, 2, "of")
        && (word_at(cursor, 3, "them") || word_at(cursor, 3, "those"))
}

/// The set anaphor a member iteration names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum IteratedSetAnaphor {
    /// `them`: the set itself.
    Pronoun,
    /// `those NOUNS`: the set, asserted to have the nominal's predicate.
    Nominal(super::ObjectPredicate),
}

/// Reads `for each of them|those NOUNS ,`. The set is resolved by the parent
/// against the program's bound set; this child only reads the anaphor.
pub(in crate::oracle::canonical) fn recognize_member_iteration_introducer(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<IteratedSetAnaphor> {
    cursor.enter("sequence.for-each-member.partitive");
    cursor.consume_phrase(&["for", "each", "of"])?;
    cursor.enter("sequence.for-each-member.set");
    let anaphor = if word_at(cursor, 0, "them") {
        cursor.consume("them")?;
        IteratedSetAnaphor::Pronoun
    } else {
        cursor.consume("those")?;
        // The nominal ends at the introducer's comma.
        let end = (0..)
            .take_while(|offset| cursor.peek_ahead(*offset).is_some())
            .find(|offset| cursor.peek_ahead(*offset) == Some(","))
            .map(|offset| cursor.position() + offset)
            .ok_or_else(|| cursor.fail("a comma closing the iterated set"))?;
        cursor.enter("sequence.for-each-member.nominal");
        let query = cursor.bounded(end, super::recognize_object_query)?;
        cursor.require(
            query.controller.is_none(),
            "an unscoped nominal restating the iterated set",
        )?;
        IteratedSetAnaphor::Nominal(query.predicate)
    };
    cursor.enter("sequence.for-each-member.separator");
    cursor.consume(",")?;
    Ok(anaphor)
}

/// True when `asserted` names objects the declared selector's predicate
/// guarantees: the same predicate, or one operand of its intersection.
pub(in crate::oracle::canonical) fn nominal_restates_selector(
    selector: &super::TargetSelector,
    asserted: &super::ObjectPredicate,
) -> bool {
    fn contains(container: &super::ObjectPredicate, member: &super::ObjectPredicate) -> bool {
        container == member
            || matches!(
                container,
                super::ObjectPredicate::Intersection(operands)
                    if operands.iter().any(|operand| contains(operand, member))
            )
    }
    match selector {
        super::TargetSelector::Object(predicate)
        | super::TargetSelector::ControlledObject { predicate, .. } => {
            contains(predicate, asserted)
        }
        _ => false,
    }
}

/// `repeat this process` / `repeat the following process` at the cursor.
pub(in crate::oracle::canonical) fn repeat_process_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "repeat")
        && ((word_at(cursor, 1, "this") && word_at(cursor, 2, "process"))
            || (word_at(cursor, 1, "the")
                && word_at(cursor, 2, "following")
                && word_at(cursor, 3, "process")))
}

/// The offset of the word `word` after an amount at the cursor, inside the
/// current clause, reading at most three amount tokens (`X`, `two`, `that
/// many`).
fn amount_end(cursor: &OracleTokenCursor<'_>, word: &str) -> Option<usize> {
    (1..=3)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":" | "("))
        })
        .find(|offset| word_at(cursor, *offset, word))
}

fn bounded_amount(
    cursor: &mut OracleTokenCursor<'_>,
    offset: usize,
) -> MachineResult<NumericExpression> {
    let end = cursor.position() + offset;
    cursor.bounded(end, |amount| {
        super::recognize_numeric_expression(amount, None)
    })
}

/// Reads one repetition instruction. Every bound or qualifier this reader
/// does not type (`for each opponent`, `for an enchantment`, `except ...`,
/// `until ...`, `any number of times`) fails here explicitly.
pub(in crate::oracle::canonical) fn recognize_repeat_process(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RepeatProcessNode> {
    cursor.enter("effect.repeat-process.verb");
    cursor.consume("repeat")?;
    if word_at(cursor, 0, "the") {
        cursor.enter("effect.repeat-process.following");
        cursor.consume_phrase(&["the", "following", "process"])?;
        cursor.enter("effect.repeat-process.following.times");
        let offset = amount_end(cursor, "times")
            .ok_or_else(|| cursor.fail("an amount followed by `times`"))?;
        let times = bounded_amount(cursor, offset)?;
        cursor.consume("times")?;
        cursor.enter("effect.repeat-process.following.terminator");
        cursor.require(
            cursor.peek() == Some("."),
            "the sentence end before the following process",
        )?;
        return Ok(RepeatProcessNode {
            process: RepeatedProcess::Following { times },
        });
    }
    cursor.enter("effect.repeat-process.anaphor");
    cursor.consume_phrase(&["this", "process"])?;
    cursor.enter("effect.repeat-process.bound");
    let additional = if word_at(cursor, 0, "once") {
        cursor.consume("once")?;
        Some(NumericExpression::from(1))
    } else if word_at(cursor, 0, "twice") {
        cursor.consume("twice")?;
        Some(NumericExpression::from(2))
    } else if let Some(offset) =
        amount_end(cursor, "more").filter(|offset| word_at(cursor, offset + 1, "times"))
    {
        let amount = bounded_amount(cursor, offset)?;
        cursor.consume_phrase(&["more", "times"])?;
        Some(amount)
    } else {
        None
    };
    cursor.enter("effect.repeat-process.complete");
    cursor.require(
        matches!(cursor.peek(), None | Some(".") | Some("(")),
        "the end of the repetition: `once`, `N more times`, or the sentence end",
    )?;
    Ok(RepeatProcessNode {
        process: RepeatedProcess::Preceding { additional },
    })
}

#[cfg(test)]
#[path = "program_iteration_tests.rs"]
mod tests;
