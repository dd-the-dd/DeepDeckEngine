//! The per-turn record of the resolving ability itself (CR 113.3, 602.1,
//! 603.1), read by the condition language and the numeric grammar.
//!
//! ```text
//! Ordinal   := "this" "is" "the" ORD ("or" ORD)* "time" Record
//!            | "it's" "the" ORD ("or" ORD)* "time"        (elided Record)
//! Threshold := "this" "ability" "has" ("been" "activated" | "resolved")
//!              THRESHOLD ("time" | "times") "this" "turn"
//! Count     := "the" "number" "of" "times" Record
//! Record    := "this" "ability" "has" ("resolved" | "been" "activated")
//!              "this" "turn"
//! ```
//!
//! The count includes the current resolution (or activation), so `the
//! third time` is `count == 3`. A list of ordinals that starts at the first
//! one (`the first or second time`) is `count <= N`, which is exact because
//! the count of a resolving ability is at least one; any other list has no
//! single comparison and is refused. The elided form (`If it's the second
//! time, ...`) restores its record from an earlier sentence of the same
//! ability that states it; without one it is refused, never guessed.
use super::super::{MachineResult, OracleTokenCursor, parse_ordinal_word};
use super::{
    AbilityHistoryMeasure, ConditionAtom, ConditionComparison, ConditionNumber,
    ConditionPlayerSubject, NumericComparisonOperator, NumericExpression, NumericQuantity,
    recognize_count_threshold, threshold_comparison,
};

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a token"))?
        .to_string();
    cursor.consume(&raw)
}

/// The record named at `offset` (`this ability has resolved|been activated
/// this turn`) and its width.
fn record_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(AbilityHistoryMeasure, usize)> {
    if !(word_is(cursor, offset, "this")
        && word_is(cursor, offset + 1, "ability")
        && word_is(cursor, offset + 2, "has"))
    {
        return None;
    }
    if word_is(cursor, offset + 3, "resolved") {
        return Some((AbilityHistoryMeasure::ResolutionsThisTurn, 4));
    }
    (word_is(cursor, offset + 3, "been") && word_is(cursor, offset + 4, "activated"))
        .then_some((AbilityHistoryMeasure::ActivationsThisTurn, 5))
}

/// The width of `the ORD (or ORD)*` at `offset` when it is followed by
/// `time`, with the ordinals read.
fn ordinal_list_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(Vec<i64>, usize)> {
    if !word_is(cursor, offset, "the") {
        return None;
    }
    let mut ordinals = Vec::new();
    let mut at = offset + 1;
    loop {
        let value = cursor.peek_ahead(at).and_then(parse_ordinal_word)?;
        ordinals.push(value);
        at += 1;
        if word_is(cursor, at, "or") {
            at += 1;
            continue;
        }
        break;
    }
    word_is(cursor, at, "time").then_some((ordinals, at + 1 - offset))
}

/// Whether an earlier sentence of the same ability states the resolution
/// record an elided `it's the ORD time` restores.
fn earlier_resolution_record(cursor: &OracleTokenCursor<'_>) -> bool {
    let earlier = cursor.token_slice(0, cursor.position());
    earlier.windows(7).any(|window| {
        let words = ["time", "this", "ability", "has", "resolved", "this", "turn"];
        window
            .iter()
            .zip(words)
            .all(|(token, word)| token.raw.eq_ignore_ascii_case(word))
    })
}

/// Lexical lookahead for the three condition forms of this module.
pub(in crate::oracle::canonical) fn ability_history_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    // `this is the ORD time RECORD`
    if word_is(cursor, 0, "this") && word_is(cursor, 1, "is") {
        return ordinal_list_at(cursor, 2)
            .is_some_and(|(_, width)| record_at(cursor, 2 + width).is_some());
    }
    // `it's the ORD time` (elided record)
    if word_is(cursor, 0, "it's") {
        return ordinal_list_at(cursor, 1).is_some_and(|(_, width)| {
            record_at(cursor, 1 + width).is_some() || earlier_resolution_record(cursor)
        });
    }
    // `this ability has resolved|been activated THRESHOLD times this turn`
    record_at(cursor, 0).is_some()
}

/// `this is the ORD [or ORD] time RECORD`, `it's the ORD time`, or `this
/// ability has (been activated | resolved) THRESHOLD time(s) this turn`.
pub(in crate::oracle::canonical) fn recognize_ability_history_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.ability-history.subject");
    if word_is(cursor, 0, "this") && word_is(cursor, 1, "is") {
        consume_current(cursor)?;
        consume_current(cursor)?;
        let (operator, value) = recognize_ordinal_time(cursor)?;
        let measure = recognize_record(cursor)?;
        return Ok(history_comparison(measure, operator, value));
    }
    if word_is(cursor, 0, "it's") {
        consume_current(cursor)?;
        let (operator, value) = recognize_ordinal_time(cursor)?;
        cursor.enter("condition.ability-history.elided-record");
        let measure = if record_at(cursor, 0).is_some() {
            recognize_record(cursor)?
        } else {
            cursor.require(
                earlier_resolution_record(cursor),
                "an earlier sentence of this ability that states `this ability has resolved this turn`",
            )?;
            AbilityHistoryMeasure::ResolutionsThisTurn
        };
        return Ok(history_comparison(measure, operator, value));
    }
    cursor.enter("condition.ability-history.record");
    let (measure, width) =
        record_at(cursor, 0).ok_or_else(|| cursor.fail("`this ability has resolved`"))?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.ability-history.threshold");
    let threshold = recognize_count_threshold(cursor, false)?;
    cursor.enter("condition.ability-history.times");
    cursor.require(
        word_is(cursor, 0, "time") || word_is(cursor, 0, "times"),
        "`time` or `times` after the threshold",
    )?;
    consume_current(cursor)?;
    cursor.enter("condition.ability-history.period");
    cursor.consume_phrase(&["this", "turn"])?;
    let (operator, value) = threshold_comparison(threshold);
    Ok(history_comparison(measure, operator, value))
}

/// `the ORD [or ORD]* time`: one comparison of the count.
fn recognize_ordinal_time(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(NumericComparisonOperator, i64)> {
    cursor.enter("condition.ability-history.ordinal");
    let (mut ordinals, width) =
        ordinal_list_at(cursor, 0).ok_or_else(|| cursor.fail("`the ORDINAL time`"))?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.require(
        ordinals.iter().all(|ordinal| *ordinal >= 1),
        "positive ordinals",
    )?;
    ordinals.sort_unstable();
    ordinals.dedup();
    if let [single] = ordinals.as_slice() {
        return Ok((NumericComparisonOperator::Equal, *single));
    }
    // The count of the resolving ability is at least one: `the first or
    // second time` is exactly `count <= 2`.
    cursor.enter("condition.ability-history.ordinal-list");
    let contiguous_from_first = ordinals
        .iter()
        .enumerate()
        .all(|(index, ordinal)| *ordinal == index as i64 + 1);
    cursor.require(
        contiguous_from_first,
        "a list of ordinals that starts at the first time and has no gap",
    )?;
    Ok((
        NumericComparisonOperator::LessThanOrEqual,
        *ordinals.last().expect("a list of two or more ordinals"),
    ))
}

/// `this ability has resolved|been activated this turn`.
fn recognize_record(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<AbilityHistoryMeasure> {
    cursor.enter("condition.ability-history.record");
    let (measure, width) = record_at(cursor, 0)
        .ok_or_else(|| cursor.fail("`this ability has resolved` or `has been activated`"))?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.ability-history.period");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(measure)
}

fn history_comparison(
    measure: AbilityHistoryMeasure,
    operator: NumericComparisonOperator,
    value: i64,
) -> ConditionAtom {
    ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(ability_history_expression(measure)),
        operator,
        right: ConditionNumber::Integer(value),
    })
}

/// The numeric quantity of one record.
pub(in crate::oracle::canonical) fn ability_history_expression(
    measure: AbilityHistoryMeasure,
) -> NumericExpression {
    NumericExpression::Quantity(Box::new(NumericQuantity::AbilityHistory(measure)))
}

/// Whether `expression` reads the resolving ability's own record, which
/// only an effect-program position (the ability's resolution) can evaluate.
pub(in crate::oracle::canonical) fn reads_ability_history(expression: &NumericExpression) -> bool {
    match expression {
        NumericExpression::Quantity(quantity) => {
            matches!(quantity.as_ref(), NumericQuantity::AbilityHistory(_))
        }
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            reads_ability_history(left) || reads_ability_history(right)
        }
        _ => false,
    }
}

/// Lexical lookahead for `the number of times RECORD`.
pub(in crate::oracle::canonical) fn ability_history_count_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "the")
        && word_is(cursor, 1, "number")
        && word_is(cursor, 2, "of")
        && word_is(cursor, 3, "times")
        && record_at(cursor, 4).is_some()
}

/// `the number of times this ability has resolved this turn`.
pub(in crate::oracle::canonical) fn recognize_ability_history_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.ability-history.count");
    cursor.consume_phrase(&["the", "number", "of", "times"])?;
    let measure = recognize_record(cursor)?;
    Ok(ability_history_expression(measure))
}

#[cfg(test)]
#[path = "ability_history_tests.rs"]
mod tests;
