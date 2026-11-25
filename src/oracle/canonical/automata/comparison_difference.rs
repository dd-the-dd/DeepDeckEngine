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
    ConditionScope, NumericComparisonOperator, NumericExpression, NumericObjectProperty,
    NumericQuantity, ObjectKind, ObjectPredicate, SelectorNumericProperty,
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

/// Lexical lookahead for `the difference between`.
pub(in crate::oracle::canonical) fn difference_between_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "the") && word_is(cursor, 1, "difference") && word_is(cursor, 2, "between")
}

/// A bare property word after `and` (`its power and toughness`): the right
/// operand restates the left operand's object with another property.
fn elliptic_property(cursor: &OracleTokenCursor<'_>) -> Option<(usize, SelectorNumericProperty)> {
    if word_is(cursor, 0, "power") {
        Some((1, SelectorNumericProperty::Power))
    } else if word_is(cursor, 0, "toughness") {
        Some((1, SelectorNumericProperty::Toughness))
    } else if word_is(cursor, 0, "mana") && word_is(cursor, 1, "value") {
        Some((2, SelectorNumericProperty::ManaValue))
    } else {
        None
    }
}

/// The same one-object property read with another property: the object of
/// `left` is kept, only the property changes. Any other operand has no
/// object to restate and is `None`.
fn restate_property(
    left: &NumericExpression,
    property: SelectorNumericProperty,
) -> Option<NumericExpression> {
    Some(match left {
        NumericExpression::BoundObjectProperty { object, .. } => {
            NumericExpression::BoundObjectProperty {
                object: *object,
                property,
            }
        }
        NumericExpression::QualifiedTargetProperty {
            object,
            required_nominal,
            ..
        } => NumericExpression::QualifiedTargetProperty {
            object: *object,
            required_nominal: required_nominal.clone(),
            property,
        },
        NumericExpression::PowerOf(object) | NumericExpression::ManaValueOf(object) => {
            match property {
                SelectorNumericProperty::Power => NumericExpression::PowerOf(*object),
                SelectorNumericProperty::ManaValue => NumericExpression::ManaValueOf(*object),
                // The typed toughness of a declared object keeps the
                // permanent nominal the bare primary gives it.
                SelectorNumericProperty::Toughness => NumericExpression::QualifiedTargetProperty {
                    object: *object,
                    required_nominal: ObjectPredicate::ObjectKind(ObjectKind::Permanent),
                    property,
                },
            }
        }
        NumericExpression::ObjectProperty { object, .. } => NumericExpression::ObjectProperty {
            object: *object,
            property: match property {
                SelectorNumericProperty::Power => NumericObjectProperty::Power,
                SelectorNumericProperty::Toughness => NumericObjectProperty::Toughness,
                SelectorNumericProperty::ManaValue => return None,
            },
        },
        _ => return None,
    })
}

/// `the difference between A and B` (CR 107.1): the absolute value of `A -
/// B`, read by the enclosing arithmetic grammar's operand reader. `POSSESSOR
/// power and toughness` restates the left operand's object with the second
/// property (`the difference between its power and toughness`).
pub(in crate::oracle::canonical) fn recognize_difference_between(
    cursor: &mut OracleTokenCursor<'_>,
    mut read_operand: impl FnMut(&mut OracleTokenCursor<'_>) -> MachineResult<NumericExpression>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.difference-between.reference");
    cursor.consume_phrase(&["the", "difference", "between"])?;
    cursor.enter("numeric.difference-between.left");
    let left = read_operand(cursor)?;
    cursor.enter("numeric.difference-between.connector");
    cursor.consume("and")?;
    cursor.enter("numeric.difference-between.right");
    let right = match elliptic_property(cursor) {
        Some((width, property)) => {
            let restated = cursor.require_value(
                restate_property(&left, property),
                "a left operand naming one object whose other property `and` restates",
            )?;
            for _ in 0..width {
                let raw = cursor.peek().unwrap_or_default().to_owned();
                cursor.consume(&raw)?;
            }
            restated
        }
        None => read_operand(cursor)?,
    };
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::Difference(left, right),
    )))
}

#[cfg(test)]
#[path = "comparison_difference_tests.rs"]
mod tests;
