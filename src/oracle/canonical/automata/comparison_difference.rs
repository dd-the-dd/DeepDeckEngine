//! `the difference` (CR 107.1): the amount by which the comparison of the
//! nearest preceding condition of the same ability holds.
//!
//! ```text
//! Difference := "the" "difference"
//! Antecedent := "if" "you" "have" ("fewer" | "less" | "more") "than" N MEASURE ","
//! ```
//!
//! The antecedent is the one comparison the nearest preceding `if` of the
//! same token stream states, re-read with the shared condition atom
//! grammar: a measure of the ability's controller against a stated number
//! (`if you have fewer than seven cards in hand, draw cards equal to the
//! difference`). Because the difference is read where that condition holds,
//! its sign is the comparison's: `fewer than N` is `N - measure`, `more
//! than N` is `measure - N`. A comparison of two players, a threshold
//! stated without `than` (`N or fewer`), or no preceding condition is
//! refused; the difference is never guessed.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ConditionAtom, ConditionComparison, ConditionContext, ConditionNumber, ConditionPlayerSubject,
    ConditionScope, NumericComparisonOperator, NumericExpression, NumericQuantity,
    recognize_condition_atom,
};

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Lexical lookahead for a bare `the difference` (not `the difference
/// between ...`, which names its own operands).
pub(in crate::oracle::canonical) fn comparison_difference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "the")
        && word_is(cursor, 1, "difference")
        && !word_is(cursor, 2, "between")
        && !word_is(cursor, 2, "in")
}

const NO_ANTECEDENT: &str = "a preceding condition of this ability comparing a measure of yours with a number stated after `than`";

/// Reads `the difference` and resolves it against the nearest preceding
/// condition of the same ability.
pub(in crate::oracle::canonical) fn recognize_comparison_difference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.difference.reference");
    cursor.consume_phrase(&["the", "difference"])?;
    cursor.enter("numeric.difference.antecedent");
    let earlier = cursor.token_slice(0, cursor.position());
    let start = earlier
        .iter()
        .rposition(|token| token.raw.eq_ignore_ascii_case("if"))
        .ok_or_else(|| cursor.fail(NO_ANTECEDENT))?;
    let end = earlier[start..]
        .iter()
        .position(|token| token.raw == ",")
        .map(|offset| start + offset)
        .ok_or_else(|| cursor.fail(NO_ANTECEDENT))?;
    let stated_than = earlier[start..end]
        .iter()
        .any(|token| token.raw.eq_ignore_ascii_case("than"));
    cursor.require(stated_than, NO_ANTECEDENT)?;
    // Re-read the condition with the shared atom grammar, as a state the
    // ability's controller has (no program antecedent is involved).
    let mut reader = OracleTokenCursor::new("comparison-difference", &earlier[..end]);
    let context = ConditionContext::standalone(ConditionScope::TriggerGuard, &[], None, false);
    let mut atom = None;
    if reader.advance_to(start + 1).is_ok()
        && let Ok(read) = recognize_condition_atom(&mut reader, &context)
        && reader.position() == end
    {
        atom = Some(read);
    }
    let atom = cursor.require_value(atom, NO_ANTECEDENT)?;
    cursor.enter("numeric.difference.comparison");
    let ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Player(measure),
        operator,
        right: ConditionNumber::Integer(value),
    }) = atom
    else {
        return Err(cursor.fail(NO_ANTECEDENT));
    };
    let measured =
        NumericExpression::Quantity(Box::new(NumericQuantity::ControllerMeasure(measure)));
    match operator {
        // `fewer than N` is `at most N - 1`: the difference is `N - measure`.
        NumericComparisonOperator::LessThanOrEqual => Ok(NumericExpression::subtract(
            NumericExpression::from(value + 1),
            measured,
        )),
        // `more than N` is `at least N + 1`: the difference is `measure - N`.
        NumericComparisonOperator::GreaterThanOrEqual => Ok(NumericExpression::subtract(
            measured,
            NumericExpression::from(value - 1),
        )),
        _ => Err(cursor.fail(NO_ANTECEDENT)),
    }
}

#[cfg(test)]
#[path = "comparison_difference_tests.rs"]
mod tests;
