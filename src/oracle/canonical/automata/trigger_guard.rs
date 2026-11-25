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

/// The words of the once-each-turn rider sentence, its period included.
const ONCE_EACH_TURN_RIDER: [&str; 7] = ["do", "this", "only", "once", "each", "turn", "."];

/// Whether the rider `Do this only once each turn.` starts at the cursor.
pub(in crate::oracle::canonical) fn once_each_turn_rider_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ONCE_EACH_TURN_RIDER
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Absolute start of a later top-level sentence `Do this only once each
/// turn.` (Ondu Spiritdancer, Donal): the rider limits the instruction of
/// the program written before it, never an instruction after it. Reminder
/// text and quoted abilities are other scopes (CR 207.2, 113.1); a rider at
/// the cursor has no instruction to limit. Lexical lookahead only.
pub(in crate::oracle::canonical) fn once_each_turn_rider_start(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        match raw {
            "\"" | "\u{201c}" | "\u{201d}" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            "." if depth == 0 && !quoted => {
                let mut probe = cursor.clone();
                if probe.advance_to(cursor.position() + offset + 1).is_ok()
                    && once_each_turn_rider_starts(&probe)
                {
                    return Some(cursor.position() + offset + 1);
                }
            }
            _ => {}
        }
        offset += 1;
    }
    None
}

/// Reads `Do this only once each turn.` and a reminder that directly
/// follows it (CR 207.2: reminder text has no rules meaning). The program
/// parent owns which instruction the rider limits; the lowering proves it is
/// exactly one optional instruction. The limit is on performing that
/// instruction: the ability still triggers and resolves, and declining it
/// leaves the instruction available later in the turn.
pub(in crate::oracle::canonical) fn recognize_once_each_turn_rider(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    cursor.enter("program.once-each-turn.rider");
    cursor.require(
        once_each_turn_rider_starts(cursor),
        "`Do this only once each turn.`",
    )?;
    cursor.consume_phrase(&ONCE_EACH_TURN_RIDER)?;
    if cursor.peek() == Some("(") {
        cursor.enter("program.once-each-turn.reminder");
        let mut depth = 0usize;
        loop {
            let raw = cursor
                .peek()
                .ok_or_else(|| cursor.fail("a closing parenthesis for the rider's reminder"))?
                .to_string();
            match raw.as_str() {
                "(" => depth += 1,
                ")" => depth = depth.saturating_sub(1),
                _ => {}
            }
            cursor.consume(&raw)?;
            if depth == 0 {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "trigger_guard_tests.rs"]
mod tests;
