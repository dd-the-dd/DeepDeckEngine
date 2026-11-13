use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PowerToughnessNode {
    pub power: i64,
    pub toughness: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PowerToughnessDeltaNode {
    pub power: i64,
    pub toughness: i64,
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
