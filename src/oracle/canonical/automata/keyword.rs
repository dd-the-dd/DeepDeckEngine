use super::super::{MachineResult, OracleTokenCursor};
use super::{ManaCostNode, recognize_mana_cost};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct KickerKeywordNode {
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KickerState {
    Keyword,
    Cost,
    ReminderOrComplete,
    ReminderOpen,
    ReminderPermission,
    ReminderCost,
    ReminderCostMatch,
    ReminderTiming,
    ReminderTerminator,
    ReminderClose,
    Complete,
}

#[derive(Default)]
struct KickerMemory {
    printed_cost: Option<ManaCostNode>,
    reminder_cost: Option<ManaCostNode>,
}

/// Recognizes a mana Kicker keyword and structurally validates its optional
/// canonical reminder sentence.
pub(in crate::oracle::canonical) fn recognize_mana_kicker(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<KickerKeywordNode> {
    let mut state = KickerState::Keyword;
    let mut memory = KickerMemory::default();
    loop {
        state = match state {
            KickerState::Keyword => {
                cursor.enter("keyword.kicker.keyword");
                cursor.consume("kicker")?;
                KickerState::Cost
            }
            KickerState::Cost => {
                cursor.enter("keyword.kicker.cost");
                memory.printed_cost = Some(recognize_mana_cost(cursor)?);
                KickerState::ReminderOrComplete
            }
            KickerState::ReminderOrComplete => {
                cursor.enter("keyword.kicker.reminder-or-complete");
                match cursor.peek() {
                    None => KickerState::Complete,
                    Some(token) if token == "(" => KickerState::ReminderOpen,
                    _ => return Err(cursor.fail("a canonical Kicker reminder or end of input")),
                }
            }
            KickerState::ReminderOpen => {
                cursor.enter("keyword.kicker.reminder.open");
                cursor.consume("(")?;
                KickerState::ReminderPermission
            }
            KickerState::ReminderPermission => {
                cursor.enter("keyword.kicker.reminder.permission");
                cursor.consume_phrase(&["you", "may", "pay", "an", "additional"])?;
                KickerState::ReminderCost
            }
            KickerState::ReminderCost => {
                cursor.enter("keyword.kicker.reminder.cost");
                memory.reminder_cost = Some(recognize_mana_cost(cursor)?);
                KickerState::ReminderCostMatch
            }
            KickerState::ReminderCostMatch => {
                cursor.enter("keyword.kicker.reminder.cost-match");
                let printed = memory
                    .printed_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the printed Kicker cost"))?;
                let reminder = memory
                    .reminder_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the reminder Kicker cost"))?;
                cursor.require(
                    printed.same_symbols(reminder),
                    "a reminder cost equal to the printed Kicker cost",
                )?;
                KickerState::ReminderTiming
            }
            KickerState::ReminderTiming => {
                cursor.enter("keyword.kicker.reminder.timing");
                cursor.consume_phrase(&["as", "you", "cast", "this", "spell"])?;
                KickerState::ReminderTerminator
            }
            KickerState::ReminderTerminator => {
                cursor.enter("keyword.kicker.reminder.terminator");
                cursor.consume(".")?;
                KickerState::ReminderClose
            }
            KickerState::ReminderClose => {
                cursor.enter("keyword.kicker.reminder.close");
                cursor.consume(")")?;
                KickerState::Complete
            }
            KickerState::Complete => {
                cursor.enter("keyword.kicker.complete");
                cursor.finish()?;
                return Ok(KickerKeywordNode {
                    cost: cursor.require_value(memory.printed_cost, "a populated Kicker cost")?,
                });
            }
        };
    }
}
