//! A replacement amount bound to the quantity of the event it replaces
//! (CR 614.1a): `that much damage plus 1`, `twice that much life`, `half that
//! damage, rounded down`. The quantity noun belongs to the event's parent;
//! operators and rounding are the shared arithmetic nodes.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EventNumericBinding, NumericAtom, NumericExpression, NumericRounding, recognize_numeric_atom,
};

fn peek_is(cursor: &OracleTokenCursor<'_>, word: &str) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn ahead_is(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// A literal integer quantity. `X` and other variables need a producer the
/// replacement frames do not declare.
pub(in crate::oracle::canonical) fn recognize_literal_quantity(
    cursor: &mut OracleTokenCursor<'_>,
    state: &'static str,
    expected: &str,
) -> MachineResult<i64> {
    let atom = recognize_numeric_atom(cursor)?;
    cursor.enter(state);
    match atom {
        NumericAtom::Integer(value) => Ok(value),
        NumericAtom::VariableX => Err(cursor.fail(expected)),
    }
}

/// Whether an event-quantity amount starts with a scaling word.
pub(in crate::oracle::canonical) fn event_quantity_scale_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek().is_some_and(|raw| {
        ["half", "double", "twice", "triple", "thrice"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
    })
}

/// A replacement amount bound to the replaced event's quantity of `noun`
/// (`damage`, `life`): `that much NOUN [plus|minus N]`, a scaled `double that
/// NOUN`, `twice that much NOUN`, `half that NOUN, rounded down`, or a fixed
/// `N NOUN`. Operators and rounding are the shared arithmetic nodes.
pub(in crate::oracle::canonical) fn recognize_event_quantity_expression(
    cursor: &mut OracleTokenCursor<'_>,
    noun: &str,
) -> MachineResult<NumericExpression> {
    cursor.enter("replacement.event-quantity.scale");
    let scale = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some(word @ ("double" | "twice")) => {
            cursor.consume(word)?;
            Some(2)
        }
        Some(word @ ("triple" | "thrice")) => {
            cursor.consume(word)?;
            Some(3)
        }
        Some("half") => {
            cursor.consume("half")?;
            Some(-2)
        }
        _ => None,
    };
    if scale.is_none() && !peek_is(cursor, "that") {
        cursor.enter("replacement.event-quantity.fixed");
        let value = recognize_literal_quantity(
            cursor,
            "replacement.event-quantity.literal",
            "a literal replacement amount",
        )?;
        cursor.consume(noun)?;
        return Ok(NumericExpression::from(value));
    }
    cursor.enter("replacement.event-quantity.anaphor");
    cursor.consume("that")?;
    if peek_is(cursor, "much") {
        cursor.consume("much")?;
    }
    cursor.consume(noun)?;
    let event_value = NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity);
    let mut expression = match scale {
        None => event_value,
        Some(-2) => {
            cursor.enter("replacement.event-quantity.rounding");
            cursor.consume_phrase(&[",", "rounded"])?;
            let rounding = if peek_is(cursor, "up") {
                cursor.consume("up")?;
                NumericRounding::Up
            } else {
                cursor.consume("down")?;
                NumericRounding::Down
            };
            // `half that damage, rounded down, to that player` / `..., instead`: the comma
            // closing the parenthetical rounding clause.
            if peek_is(cursor, ",") && (ahead_is(cursor, 1, "to") || ahead_is(cursor, 1, "instead"))
            {
                cursor.consume(",")?;
            }
            NumericExpression::divide(event_value, 2, rounding)
                .expect("the literal divisor two is positive")
        }
        Some(factor) => NumericExpression::multiply(NumericExpression::from(factor), event_value),
    };
    cursor.enter("replacement.event-quantity.offset");
    if peek_is(cursor, "plus") || peek_is(cursor, "minus") {
        let add = peek_is(cursor, "plus");
        cursor.consume(if add { "plus" } else { "minus" })?;
        let offset = recognize_literal_quantity(
            cursor,
            "replacement.event-quantity.literal",
            "a literal amount added to the event's quantity",
        )?;
        let offset = NumericExpression::from(offset);
        expression = if add {
            NumericExpression::add(expression, offset)
        } else {
            NumericExpression::subtract(expression, offset)
        };
    }
    Ok(expression)
}
