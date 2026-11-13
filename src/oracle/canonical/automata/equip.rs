use super::super::{MachineResult, OracleTokenCursor};
use super::{ManaCostNode, recognize_mana_cost};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EquipKeywordNode {
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EquipState {
    Keyword,
    Cost,
    ReminderOrComplete,
    ReminderOpen,
    ReminderCost,
    ReminderCostMatch,
    ReminderSeparator,
    ReminderAttach,
    ReminderRelation,
    ReminderTarget,
    ReminderObject,
    ReminderPlayer,
    ReminderControl,
    ReminderAttachTerminator,
    ReminderKeyword,
    ReminderOnly,
    ReminderAs,
    ReminderArticle,
    ReminderTiming,
    ReminderTimingTerminator,
    ReminderClose,
    Complete,
}

#[derive(Default)]
struct EquipMemory {
    printed_cost: Option<ManaCostNode>,
    reminder_cost: Option<ManaCostNode>,
}

/// Recognizes a mana-cost Equip keyword and structurally validates its optional
/// canonical attach-and-timing reminder without reparsing a text slice.
pub(in crate::oracle::canonical) fn recognize_mana_equip(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EquipKeywordNode> {
    let mut state = EquipState::Keyword;
    let mut memory = EquipMemory::default();
    loop {
        state = match state {
            EquipState::Keyword => {
                cursor.enter("keyword.equip.keyword");
                cursor.consume("equip")?;
                EquipState::Cost
            }
            EquipState::Cost => {
                cursor.enter("keyword.equip.cost");
                memory.printed_cost = Some(recognize_mana_cost(cursor)?);
                EquipState::ReminderOrComplete
            }
            EquipState::ReminderOrComplete => {
                cursor.enter("keyword.equip.reminder-or-complete");
                match cursor.peek() {
                    None => EquipState::Complete,
                    Some("(") => EquipState::ReminderOpen,
                    _ => return Err(cursor.fail("a canonical Equip reminder or end of input")),
                }
            }
            EquipState::ReminderOpen => {
                cursor.enter("keyword.equip.reminder.open");
                cursor.consume("(")?;
                EquipState::ReminderCost
            }
            EquipState::ReminderCost => {
                cursor.enter("keyword.equip.reminder.cost");
                memory.reminder_cost = Some(recognize_mana_cost(cursor)?);
                EquipState::ReminderCostMatch
            }
            EquipState::ReminderCostMatch => {
                cursor.enter("keyword.equip.reminder.cost-match");
                let printed = memory
                    .printed_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the printed Equip cost"))?;
                let reminder = memory
                    .reminder_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the reminder Equip cost"))?;
                cursor.require(
                    printed.same_symbols(reminder),
                    "a reminder cost equal to the printed Equip cost",
                )?;
                EquipState::ReminderSeparator
            }
            EquipState::ReminderSeparator => {
                cursor.enter("keyword.equip.reminder.separator");
                cursor.consume(":")?;
                EquipState::ReminderAttach
            }
            EquipState::ReminderAttach => {
                cursor.enter("keyword.equip.reminder.attach");
                cursor.consume("attach")?;
                EquipState::ReminderRelation
            }
            EquipState::ReminderRelation => {
                cursor.enter("keyword.equip.reminder.relation");
                cursor.consume("to")?;
                EquipState::ReminderTarget
            }
            EquipState::ReminderTarget => {
                cursor.enter("keyword.equip.reminder.target");
                cursor.consume("target")?;
                EquipState::ReminderObject
            }
            EquipState::ReminderObject => {
                cursor.enter("keyword.equip.reminder.object");
                cursor.consume("creature")?;
                EquipState::ReminderPlayer
            }
            EquipState::ReminderPlayer => {
                cursor.enter("keyword.equip.reminder.player");
                cursor.consume("you")?;
                EquipState::ReminderControl
            }
            EquipState::ReminderControl => {
                cursor.enter("keyword.equip.reminder.control");
                cursor.consume("control")?;
                EquipState::ReminderAttachTerminator
            }
            EquipState::ReminderAttachTerminator => {
                cursor.enter("keyword.equip.reminder.attach-terminator");
                cursor.consume(".")?;
                EquipState::ReminderKeyword
            }
            EquipState::ReminderKeyword => {
                cursor.enter("keyword.equip.reminder.keyword");
                cursor.consume("equip")?;
                EquipState::ReminderOnly
            }
            EquipState::ReminderOnly => {
                cursor.enter("keyword.equip.reminder.only");
                cursor.consume("only")?;
                EquipState::ReminderAs
            }
            EquipState::ReminderAs => {
                cursor.enter("keyword.equip.reminder.as");
                cursor.consume("as")?;
                EquipState::ReminderArticle
            }
            EquipState::ReminderArticle => {
                cursor.enter("keyword.equip.reminder.article");
                cursor.consume("a")?;
                EquipState::ReminderTiming
            }
            EquipState::ReminderTiming => {
                cursor.enter("keyword.equip.reminder.timing");
                cursor.consume("sorcery")?;
                EquipState::ReminderTimingTerminator
            }
            EquipState::ReminderTimingTerminator => {
                cursor.enter("keyword.equip.reminder.timing-terminator");
                cursor.consume(".")?;
                EquipState::ReminderClose
            }
            EquipState::ReminderClose => {
                cursor.enter("keyword.equip.reminder.close");
                cursor.consume(")")?;
                EquipState::Complete
            }
            EquipState::Complete => {
                cursor.enter("keyword.equip.complete");
                cursor.finish()?;
                return Ok(EquipKeywordNode {
                    cost: cursor.require_value(memory.printed_cost, "a populated Equip cost")?,
                });
            }
        };
    }
}
