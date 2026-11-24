//! One characteristic-defining power/toughness frame (CR 604.3, 613.4a):
//!
//! ```text
//! CDA       := Source Stats Copula Amount ("and" "its" Stat Copula Amount)? "."
//! Stats     := "power" | "toughness" | "power" "and" "toughness"
//! Copula    := "is" "equal" "to"                 (one stat)
//!            | "are" "each" "equal" "to"         (both stats)
//! Amount    := the shared `equal to` numeric expression
//! ```
//!
//! The source reference, the stat selection and the amount are independent
//! children; the second clause may read the first amount through the shared
//! prior-number binding (`that number plus 1`). The frame never chooses a
//! count family itself: an amount the shared numeric grammar cannot read
//! fails there, with its own typed diagnostic.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, NumericExpression, recognize_distinct_card_type_count,
    recognize_equal_to_amount_expression, recognize_numeric_expression, recognize_source_reference,
    registered_possessive_object, with_prior_number,
};

/// The stat(s) a characteristic-defining ability defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DefinedStats {
    Power,
    Toughness,
    PowerAndToughness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PowerToughnessCharacteristicNode {
    pub object: LocalObjectReference,
    /// `None` when the ability does not define power (CR 604.3: the printed
    /// value stays).
    pub power: Option<NumericExpression>,
    pub toughness: Option<NumericExpression>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Source,
    Stats,
    Copula,
    Amount,
    SecondClause,
    Terminator,
    Complete,
}

#[derive(Default)]
struct Frame {
    object: Option<LocalObjectReference>,
    stats: Option<DefinedStats>,
    power: Option<NumericExpression>,
    toughness: Option<NumericExpression>,
}

/// The token index of the boundary of a first amount: the sentence period or
/// the `and its STAT` connector of the second clause, outside parentheses.
fn amount_end(cursor: &OracleTokenCursor<'_>) -> usize {
    let mut offset = 0;
    let mut depth = 0_usize;
    loop {
        match cursor.peek_ahead(offset) {
            None => return cursor.position() + offset,
            Some("(") => depth += 1,
            Some(")") => depth = depth.saturating_sub(1),
            Some(".") if depth == 0 => return cursor.position() + offset,
            Some(raw)
                if depth == 0
                    && raw.eq_ignore_ascii_case("and")
                    && cursor
                        .peek_ahead(offset + 1)
                        .is_some_and(|word| word.eq_ignore_ascii_case("its"))
                    && cursor.peek_ahead(offset + 2).is_some_and(|word| {
                        word.eq_ignore_ascii_case("power") || word.eq_ignore_ascii_case("toughness")
                    }) =>
            {
                return cursor.position() + offset;
            }
            Some(_) => {}
        }
        offset += 1;
    }
}

fn read_stats(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<DefinedStats> {
    cursor.enter("characteristic.power-toughness.stats");
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(cursor, 0, "power") && word(cursor, 1, "and") && word(cursor, 2, "toughness") {
        cursor.consume_phrase(&["power", "and", "toughness"])?;
        Ok(DefinedStats::PowerAndToughness)
    } else if word(cursor, 0, "power") {
        cursor.consume("power")?;
        Ok(DefinedStats::Power)
    } else {
        cursor.consume("toughness")?;
        Ok(DefinedStats::Toughness)
    }
}

/// `is equal to` after one stat; `are each equal to` after both.
fn read_copula(cursor: &mut OracleTokenCursor<'_>, stats: DefinedStats) -> MachineResult<()> {
    cursor.enter("characteristic.power-toughness.equality");
    match stats {
        DefinedStats::PowerAndToughness => cursor.consume_phrase(&["are", "each"])?,
        DefinedStats::Power | DefinedStats::Toughness => cursor.consume("is")?,
    }
    Ok(())
}

/// The amount after the copula, through the shared `equal to` grammar. The
/// distinct card-type count keeps its own child, tried when the shared
/// definition grammar does not read the bounded amount.
fn read_amount(
    cursor: &mut OracleTokenCursor<'_>,
    object: LocalObjectReference,
) -> MachineResult<NumericExpression> {
    cursor.enter("characteristic.power-toughness.amount");
    cursor.consume_phrase(&["equal", "to"])?;
    let end = amount_end(cursor);
    let mut trial = cursor.clone();
    match trial.bounded(end, |child| {
        recognize_equal_to_amount_expression(child, Some(object))
    }) {
        Ok(amount) => {
            *cursor = trial;
            Ok(amount)
        }
        Err(shared) => {
            let mut distinct = cursor.clone();
            match distinct.bounded(end, recognize_distinct_card_type_count) {
                Ok(amount) => {
                    *cursor = distinct;
                    Ok(amount)
                }
                Err(_) => Err(shared),
            }
        }
    }
}

pub(in crate::oracle::canonical) fn characteristic_defining_power_toughness_starts(
    tokens: &[crate::oracle::model::RecognizedEntity],
) -> bool {
    // A stat word, then the copula and `equal to`, after the source: the
    // anchors of this frame only, never a count family or a card name.
    tokens.windows(4).any(|window| {
        (window[0].raw.eq_ignore_ascii_case("power")
            || window[0].raw.eq_ignore_ascii_case("toughness"))
            && ((window[1].raw.eq_ignore_ascii_case("is")
                && window[2].raw.eq_ignore_ascii_case("equal")
                && window[3].raw.eq_ignore_ascii_case("to"))
                || (window[1].raw.eq_ignore_ascii_case("are")
                    && window[2].raw.eq_ignore_ascii_case("each")
                    && window[3].raw.eq_ignore_ascii_case("equal")))
    })
}

/// Recognizes one complete characteristic-defining power/toughness ability.
pub(in crate::oracle::canonical) fn recognize_characteristic_defining_power_toughness(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<PowerToughnessCharacteristicNode> {
    let mut state = State::Source;
    let mut frame = Frame::default();
    loop {
        state = match state {
            State::Source => {
                cursor.enter("characteristic.power-toughness.source");
                // `This creature's power`: the source named by `this` and a
                // registered possessive object word, the same source
                // reference a numeric `this creature's power` reads.
                let possessive_this = cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                    && cursor
                        .token_slice(cursor.position() + 1, cursor.position() + 2)
                        .first()
                        .is_some_and(|token| {
                            token.token_type == "possessiveTypeWord"
                                && registered_possessive_object(&token.raw)
                        });
                frame.object = Some(if possessive_this {
                    cursor.enter("characteristic.power-toughness.source.possessive");
                    cursor.consume("this")?;
                    cursor.consume_type("possessiveTypeWord")?;
                    LocalObjectReference::SourceSelf
                } else {
                    recognize_source_reference(cursor, name_links, name_failure)?
                });
                State::Stats
            }
            State::Stats => {
                frame.stats = Some(read_stats(cursor)?);
                State::Copula
            }
            State::Copula => {
                let stats = cursor.require_value(frame.stats, "the defined stats")?;
                read_copula(cursor, stats)?;
                State::Amount
            }
            State::Amount => {
                let object = cursor.require_value(frame.object, "a characteristic object")?;
                let amount = read_amount(cursor, object)?;
                match cursor.require_value(frame.stats, "the defined stats")? {
                    DefinedStats::Power => frame.power = Some(amount),
                    DefinedStats::Toughness => frame.toughness = Some(amount),
                    DefinedStats::PowerAndToughness => {
                        frame.power = Some(amount.clone());
                        frame.toughness = Some(amount);
                    }
                }
                State::SecondClause
            }
            State::SecondClause => {
                cursor.enter("characteristic.power-toughness.second-clause");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    let first = cursor.require_value(frame.stats, "the defined stats")?;
                    cursor.consume_phrase(&["and", "its"])?;
                    let second = read_stats(cursor)?;
                    cursor.require(
                        matches!(
                            (first, second),
                            (DefinedStats::Power, DefinedStats::Toughness)
                                | (DefinedStats::Toughness, DefinedStats::Power)
                        ),
                        "the other stat in the second clause",
                    )?;
                    read_copula(cursor, second)?;
                    cursor.consume_phrase(&["equal", "to"])?;
                    // `that number plus 1`: the shared arithmetic grammar
                    // reads the first amount through the prior number.
                    let prior = cursor.require_value(
                        frame.power.clone().or(frame.toughness.clone()),
                        "a prior numeric-expression binding for `that number`",
                    )?;
                    let object = cursor.require_value(frame.object, "a characteristic object")?;
                    let amount = with_prior_number(cursor, prior, |cursor| {
                        if cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("that"))
                        {
                            recognize_numeric_expression(cursor, None)
                        } else {
                            recognize_equal_to_amount_expression(cursor, Some(object))
                        }
                    })?;
                    match second {
                        DefinedStats::Power => frame.power = Some(amount),
                        _ => frame.toughness = Some(amount),
                    }
                }
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("characteristic.power-toughness.terminator");
                cursor.consume(".")?;
                // A final parenthesized reminder (`(Each {W} in the mana
                // costs ... counts toward your devotion to white.)`) restates
                // the rules of the amount; it must close the ability.
                if cursor.peek() == Some("(") {
                    cursor.enter("characteristic.power-toughness.reminder");
                    let close = cursor
                        .token_slice(cursor.position(), usize::MAX)
                        .iter()
                        .position(|token| token.raw == ")")
                        .ok_or_else(|| cursor.fail("a closed reminder parenthesis"))?;
                    cursor.advance_to(cursor.position() + close + 1)?;
                }
                State::Complete
            }
            State::Complete => {
                cursor.enter("characteristic.power-toughness.complete");
                cursor.finish()?;
                return Ok(PowerToughnessCharacteristicNode {
                    object: cursor.require_value(frame.object, "a characteristic object")?,
                    power: frame.power,
                    toughness: frame.toughness,
                });
            }
        };
    }
}
