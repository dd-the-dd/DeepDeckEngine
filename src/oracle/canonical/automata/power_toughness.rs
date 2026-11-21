use super::super::{MachineResult, OracleTokenCursor};
use super::{NumericExpression, recognize_numeric_expression};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PowerToughnessNode {
    pub power: i64,
    pub toughness: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PowerToughnessDeltaNode<Amount = i64> {
    pub power: Amount,
    pub toughness: Amount,
}

pub(in crate::oracle::canonical) fn signed_power_toughness_from_raw(
    raw: &str,
) -> Option<PowerToughnessDeltaNode> {
    let (power, toughness) = raw.split_once('/')?;
    if !matches!(power.as_bytes().first(), Some(b'+') | Some(b'-'))
        || !matches!(toughness.as_bytes().first(), Some(b'+') | Some(b'-'))
    {
        return None;
    }
    Some(PowerToughnessDeltaNode {
        power: power.parse().ok()?,
        toughness: toughness.parse().ok()?,
    })
}

/// Recognizes a literal signed power/toughness delta as one shared semantic
/// atom. Parents retain ownership of the affected objects and duration.
pub(in crate::oracle::canonical) fn recognize_signed_power_toughness_delta(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PowerToughnessDeltaNode> {
    cursor.enter("characteristic.power-toughness.delta");
    let token_index = cursor.position();
    let token = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("a signed literal power/toughness delta"))?;
    cursor.require(
        token.token_type == "counterValue",
        "a signed literal power/toughness delta",
    )?;
    let delta = signed_power_toughness_from_raw(&token.raw).ok_or_else(|| {
        cursor.fail_with_span(
            "a signed literal power/toughness delta",
            token_index,
            token.span_start,
            token.span_end,
            token.raw.clone(),
        )
    })?;
    cursor.advance_to(token_index + 1)?;
    cursor.enter("characteristic.power-toughness.delta.complete");
    Ok(delta)
}

/// Each signed component delegates to the shared arithmetic reader. A compact
/// literal pair retains its original lexical and numeric representation.
pub(in crate::oracle::canonical) fn recognize_signed_numeric_power_toughness_delta(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PowerToughnessDeltaNode<NumericExpression>> {
    if cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "counterValue")
    {
        let literal = recognize_signed_power_toughness_delta(cursor)?;
        return Ok(PowerToughnessDeltaNode {
            power: literal.power.into(),
            toughness: literal.toughness.into(),
        });
    }
    cursor.enter("characteristic.power-toughness.numeric-sign");
    let negative_power = recognize_delta_sign(cursor)?;
    let start = cursor.position();
    let mut end = start;
    let mut depth = 0_usize;
    loop {
        match cursor.peek_ahead(end - start) {
            Some("/") if depth == 0 => break,
            Some("(") => depth += 1,
            Some(")") if depth > 0 => depth -= 1,
            Some("." | ";" | ":" | ",") | None => {
                return Err(cursor.fail("a slash between two signed numeric stat components"));
            }
            Some(_) => {}
        }
        end += 1;
    }
    let power = cursor.bounded(end, |child| recognize_numeric_expression(child, None))?;
    cursor.enter("characteristic.power-toughness.numeric-separator");
    cursor.consume_type("statPairSeparator")?;
    let negative_toughness = recognize_delta_sign(cursor)?;
    let toughness = recognize_numeric_expression(cursor, None)?;
    Ok(PowerToughnessDeltaNode {
        power: signed_expression(power, negative_power),
        toughness: signed_expression(toughness, negative_toughness),
    })
}

fn recognize_delta_sign(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    cursor.enter("characteristic.power-toughness.numeric-sign");
    let negative = match cursor.peek() {
        Some("+") => false,
        Some("-") => true,
        _ => return Err(cursor.fail("an explicit plus or minus before each stat component")),
    };
    cursor.consume_type("separator")?;
    Ok(negative)
}

fn signed_expression(expression: NumericExpression, negative: bool) -> NumericExpression {
    if negative {
        NumericExpression::subtract(NumericExpression::from(0), expression)
    } else {
        expression
    }
}

pub(in crate::oracle::canonical) fn recognize_literal_power_toughness(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PowerToughnessNode> {
    cursor.enter("characteristic.power-toughness.literal");
    let token_index = cursor.position();
    let token = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("nonnegative literal power/toughness"))?;
    let parsed = token
        .raw
        .split_once('/')
        .and_then(|(power, toughness)| {
            Some((power.parse::<i64>().ok()?, toughness.parse::<i64>().ok()?))
        })
        .filter(|(power, toughness)| *power >= 0 && *toughness >= 0);
    let Some((power, toughness)) = parsed else {
        return Err(cursor.fail_with_span(
            "nonnegative literal power/toughness",
            token_index,
            token.span_start,
            token.span_end,
            token.raw.clone(),
        ));
    };
    cursor.require(
        token.token_type == "counterValue",
        "a literal power/toughness token",
    )?;
    cursor.advance_to(token_index + 1)?;
    cursor.enter("characteristic.power-toughness.complete");
    Ok(PowerToughnessNode { power, toughness })
}

#[cfg(test)]
#[path = "power_toughness_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "numeric_stat_delta_tests.rs"]
mod numeric_tests;
