use super::super::{MachineResult, OracleTokenCursor};
use super::{InvestigateEffectNode, LocalPlayerReference};

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
    cursor.enter("effect.investigate.complete");
    Ok(InvestigateEffectNode {
        player: LocalPlayerReference::Controller,
    })
}

#[cfg(test)]
#[path = "investigate_tests.rs"]
mod tests;
