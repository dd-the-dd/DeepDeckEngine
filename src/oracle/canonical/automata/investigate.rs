use super::super::{MachineResult, OracleTokenCursor};
use super::{InvestigateEffectNode, LocalPlayerReference, NumericExpression};

/// Recognizes the atomic investigate action. Reminder text and triggered-
/// ability punctuation remain owned by the surrounding ability envelope.
pub(in crate::oracle::canonical) fn recognize_investigate_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<InvestigateEffectNode> {
    cursor.enter("effect.investigate.action");
    let action_index = cursor.position();
    let action = cursor.consume_type("actionWord")?;
    if !action.raw.eq_ignore_ascii_case("investigate") {
        return Err(cursor.fail_with_span(
            "the `investigate` action",
            action_index,
            action.span_start,
            action.span_end,
            action.raw.as_str(),
        ));
    }
    // `investigate twice` / `investigate X times`: the shared repetition
    // adjunct; each investigation is its own action (CR 701.16a).
    let times = if super::repetition_adjunct_starts(cursor) {
        super::recognize_repetition_adjunct(cursor)?
    } else {
        NumericExpression::from(1_i64)
    };
    cursor.enter("effect.investigate.complete");
    Ok(InvestigateEffectNode {
        player: LocalPlayerReference::Controller,
        times,
    })
}

#[cfg(test)]
#[path = "investigate_tests.rs"]
mod tests;
