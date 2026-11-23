use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ConditionContext, ConditionScope, SentenceConditionContext, SentenceConditionNode,
    recognize_boolean_condition_list_before_separator, recognize_condition_atom,
};

/// Reads the optional `if CONDITION` after a complete trigger event (the
/// intervening `if`, CR 603.4). The triggering-ability parent owns the comma
/// separating guard and effect body. Every atom is read by the one condition
/// atom reader with the trigger-guard allow-list: `it` and `this CARD`
/// denote only the ability's source, no declared target or card result
/// exists before the effect program, and no earlier action's outcome can be
/// read. Unsupported atoms remain explicit failures.
pub(in crate::oracle::canonical) fn recognize_trigger_guard(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<Option<SentenceConditionNode>> {
    if !cursor
        .peek()
        .is_some_and(|word| word.eq_ignore_ascii_case("if"))
    {
        return Ok(None);
    }

    cursor.enter("condition.trigger-guard.if");
    cursor.consume("if")?;
    cursor.enter("condition.trigger-guard.boolean-program");
    let context = ConditionContext::program(ConditionScope::TriggerGuard, context);
    let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_condition_atom(cursor, &context)
    })?;
    cursor.enter("condition.trigger-guard.parent-separator");
    cursor.require(
        cursor.peek() == Some(","),
        "a comma owned by the triggered-ability parent after the complete guard",
    )?;
    cursor.enter("condition.trigger-guard.complete");
    Ok(Some(condition))
}

#[cfg(test)]
#[path = "trigger_guard_tests.rs"]
mod tests;
