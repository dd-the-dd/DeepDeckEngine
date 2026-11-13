use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::LoyaltyCostNode;

/// Recognizes the signed loyalty payment prefix of a planeswalker ability.
/// The following colon and effect body remain owned by the activated-ability
/// envelope.
pub(in crate::oracle::canonical) fn recognize_loyalty_cost(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LoyaltyCostNode> {
    cursor.enter("cost.loyalty.sign-or-magnitude");
    let sign = match cursor.peek() {
        Some("+") => {
            cursor.consume("+")?;
            1
        }
        Some("-" | "−") => {
            let raw = cursor.peek().expect("matched sign").to_string();
            cursor.consume(&raw)?;
            -1
        }
        _ => 1,
    };

    cursor.enter("cost.loyalty.magnitude");
    let token_index = cursor.position();
    let token = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("a literal loyalty magnitude"))?;
    let magnitude = parse_number_word(&token.raw)
        .filter(|value| *value >= 0)
        .ok_or_else(|| cursor.fail("a nonnegative literal loyalty magnitude"))?;
    cursor.advance_to(token_index + 1)?;
    cursor.enter("cost.loyalty.complete");
    Ok(LoyaltyCostNode {
        delta: sign * magnitude,
    })
}

#[cfg(test)]
#[path = "loyalty_tests.rs"]
mod tests;
