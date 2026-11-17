use super::super::{MachineResult, OracleTokenCursor};
use super::{
    BooleanConditionNode, LocalConditionNode, recognize_boolean_condition_list_before_separator,
    recognize_controlled_permanent_count_query_with,
    recognize_controlled_permanent_presence_query_with,
    recognize_object_query_before_condition_connector,
};

/// Reads the optional `if CONDITION` after a complete trigger event. The
/// triggering-ability parent owns the comma separating guard and effect body.
/// No effect text is reconstructed, and unsupported atoms remain explicit.
pub(in crate::oracle::canonical) fn recognize_trigger_guard(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<BooleanConditionNode<LocalConditionNode>>> {
    if !cursor
        .peek()
        .is_some_and(|word| word.eq_ignore_ascii_case("if"))
    {
        return Ok(None);
    }

    cursor.enter("condition.trigger-guard.if");
    cursor.consume("if")?;
    cursor.enter("condition.trigger-guard.boolean-program");
    let condition =
        recognize_boolean_condition_list_before_separator(cursor, recognize_trigger_guard_atom)?;
    cursor.enter("condition.trigger-guard.parent-separator");
    cursor.require(
        cursor.peek() == Some(","),
        "a comma owned by the triggered-ability parent after the complete guard",
    )?;
    cursor.enter("condition.trigger-guard.complete");
    Ok(Some(condition))
}

/// One controller permanent-presence or fixed-count atom, shared by every
/// parent that owns an `if` Boolean program (trigger guards, cost adjustments).
pub(in crate::oracle::canonical) fn recognize_trigger_guard_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalConditionNode> {
    cursor.enter("condition.trigger-guard.atom-kind");
    cursor.require(
        cursor
            .peek()
            .is_some_and(|word| word.eq_ignore_ascii_case("you"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|word| word.eq_ignore_ascii_case("control")),
        "a supported controller permanent-presence or fixed-count condition",
    )?;
    let presence = cursor.peek_ahead(2).is_some_and(|word| {
        word.eq_ignore_ascii_case("a")
            || word.eq_ignore_ascii_case("an")
            || word.eq_ignore_ascii_case("another")
    });
    if presence {
        recognize_controlled_permanent_presence_query_with(
            cursor,
            recognize_object_query_before_condition_connector,
        )
    } else {
        recognize_controlled_permanent_count_query_with(
            cursor,
            recognize_object_query_before_condition_connector,
        )
        .map(LocalConditionNode::ControlledPermanentCount)
    }
}

#[cfg(test)]
#[path = "trigger_guard_tests.rs"]
mod tests;
