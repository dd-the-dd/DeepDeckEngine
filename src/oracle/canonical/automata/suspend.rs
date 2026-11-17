//! Suspend N—[cost] (CR 702.62a): a keyword envelope around a positive time
//! counter count and the shared typed mana-cost child. The optional reminder
//! is validated against those parameters; it never supplies the semantics.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{ManaCostNode, recognize_mana_cost};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SuspendKeywordNode {
    pub time_counters: i64,
    pub cost: ManaCostNode,
    /// The reminder states the creature-haste clause. It is informative only:
    /// the runtime expansion grants haste to any creature spell cast this way.
    pub reminder_mentions_haste: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SuspendState {
    Keyword,
    Count,
    Dash,
    Cost,
    Envelope,
    ReminderAlternative,
    ReminderPayment,
    ReminderExile,
    ReminderUpkeep,
    ReminderLastRemoved,
    ReminderHaste,
    ReminderClose,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_suspend_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SuspendKeywordNode> {
    let mut state = SuspendState::Keyword;
    let mut time_counters = None;
    let mut cost: Option<ManaCostNode> = None;
    let mut reminder_mentions_haste = false;
    loop {
        state = match state {
            SuspendState::Keyword => {
                cursor.enter("keyword.suspend.keyword");
                cursor.consume("suspend")?;
                SuspendState::Count
            }
            SuspendState::Count => {
                cursor.enter("keyword.suspend.count");
                let count = cursor
                    .peek()
                    .filter(|raw| !raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit()))
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .filter(|count| *count > 0);
                let count = cursor.require_value(
                    count,
                    "a fixed positive time-counter count; Suspend X requires a casting-X producer and runtime proof",
                )?;
                cursor.advance_to(cursor.position() + 1)?;
                time_counters = Some(count);
                SuspendState::Dash
            }
            SuspendState::Dash => {
                cursor.enter("keyword.suspend.dash");
                cursor.consume("\u{2014}")?;
                SuspendState::Cost
            }
            SuspendState::Cost => {
                cursor.enter("keyword.suspend.cost");
                let mana = recognize_mana_cost(cursor)?;
                mana.fixed_runtime_mana_value()
                    .map_err(|error| cursor.fail(error.expected()))?;
                cost = Some(mana);
                SuspendState::Envelope
            }
            SuspendState::Envelope => {
                cursor.enter("keyword.suspend.envelope");
                match cursor.peek() {
                    None => SuspendState::Complete,
                    Some("(") => {
                        cursor.consume("(")?;
                        SuspendState::ReminderAlternative
                    }
                    _ => {
                        return Err(cursor
                            .fail("the canonical Suspend reminder or end of the keyword ability"));
                    }
                }
            }
            SuspendState::ReminderAlternative => {
                cursor.enter("keyword.suspend.reminder.alternative");
                cursor.consume_phrase(&[
                    "rather", "than", "cast", "this", "card", "from", "your", "hand", ",",
                ])?;
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
                {
                    cursor.consume_phrase(&["you", "may"])?;
                }
                SuspendState::ReminderPayment
            }
            SuspendState::ReminderPayment => {
                cursor.enter("keyword.suspend.reminder.payment");
                cursor.consume("pay")?;
                let reminder_cost = recognize_mana_cost(cursor)?;
                let printed = cursor.require_value(cost.as_ref(), "the printed Suspend cost")?;
                cursor.require(
                    reminder_cost.same_symbols(printed),
                    "a reminder payment equal to the printed Suspend cost",
                )?;
                SuspendState::ReminderExile
            }
            SuspendState::ReminderExile => {
                cursor.enter("keyword.suspend.reminder.exile");
                cursor.consume_phrase(&["and", "exile", "it", "with"])?;
                let printed =
                    cursor.require_value(time_counters, "the printed time-counter count")?;
                let stated = cursor
                    .peek()
                    .and_then(|raw| raw.parse::<i64>().ok().or_else(|| parse_number_word(raw)));
                cursor.require(
                    stated == Some(printed),
                    "a reminder time-counter count equal to the printed Suspend count",
                )?;
                cursor.advance_to(cursor.position() + 1)?;
                cursor.consume("time")?;
                cursor.consume(if printed == 1 { "counter" } else { "counters" })?;
                cursor.consume_phrase(&["on", "it", "."])?;
                SuspendState::ReminderUpkeep
            }
            SuspendState::ReminderUpkeep => {
                cursor.enter("keyword.suspend.reminder.upkeep");
                cursor.consume_phrase(&[
                    "at",
                    "the",
                    "beginning",
                    "of",
                    "your",
                    "upkeep",
                    ",",
                    "remove",
                    "a",
                    "time",
                    "counter",
                    ".",
                ])?;
                SuspendState::ReminderLastRemoved
            }
            SuspendState::ReminderLastRemoved => {
                cursor.enter("keyword.suspend.reminder.last-removed");
                cursor.consume_phrase(&[
                    "when", "the", "last", "is", "removed", ",", "you", "may", "cast", "it",
                    "without", "paying", "its", "mana", "cost", ".",
                ])?;
                SuspendState::ReminderHaste
            }
            SuspendState::ReminderHaste => {
                cursor.enter("keyword.suspend.reminder.haste");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
                {
                    cursor.consume_phrase(&["it", "has", "haste", "."])?;
                    reminder_mentions_haste = true;
                }
                SuspendState::ReminderClose
            }
            SuspendState::ReminderClose => {
                cursor.enter("keyword.suspend.reminder.close");
                cursor.consume(")")?;
                SuspendState::Complete
            }
            SuspendState::Complete => {
                cursor.enter("keyword.suspend.complete");
                cursor.finish()?;
                return Ok(SuspendKeywordNode {
                    time_counters: cursor
                        .require_value(time_counters, "the retained Suspend time-counter count")?,
                    cost: cursor.require_value(cost, "the retained fixed Suspend mana cost")?,
                    reminder_mentions_haste,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "suspend_tests.rs"]
mod tests;
