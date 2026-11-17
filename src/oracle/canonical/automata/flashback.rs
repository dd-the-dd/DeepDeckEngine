//! A keyword envelope around the shared typed cost child, never a spell effect.
use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalActivationCostNode, ManaCostNode, recognize_activation_cost_sequence};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct FlashbackKeywordNode {
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlashbackState {
    Keyword,
    Cost,
    CostSupport,
    Envelope,
    ReminderPermission,
    ReminderExile,
    ReminderClose,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_flashback_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<FlashbackKeywordNode> {
    let mut state = FlashbackState::Keyword;
    let mut costs = None;
    let mut printed_mana = None;
    loop {
        state = match state {
            FlashbackState::Keyword => {
                cursor.enter("keyword.flashback.keyword");
                cursor.consume("flashback")?;
                if cursor.peek() == Some("\u{2014}") {
                    cursor.consume("\u{2014}")?;
                }
                FlashbackState::Cost
            }
            FlashbackState::Cost => {
                cursor.enter("keyword.flashback.cost");
                costs = Some(recognize_activation_cost_sequence(cursor)?);
                FlashbackState::CostSupport
            }
            FlashbackState::CostSupport => {
                cursor.enter("keyword.flashback.cost-support");
                let costs = cursor.require_value(costs.take(), "a shared typed payment cost")?;
                let [LocalActivationCostNode::PayMana(mana)] = costs.costs.as_slice() else {
                    return Err(cursor.fail("one mana payment; nonmana and composite Flashback costs require their own runtime proof"));
                };
                mana.fixed_runtime_mana_value()
                    .map_err(|error| cursor.fail(error.expected()))?;
                printed_mana = Some(mana.clone());
                FlashbackState::Envelope
            }
            FlashbackState::Envelope => {
                cursor.enter("keyword.flashback.envelope");
                if cursor.peek() == Some(".") {
                    cursor.consume(".")?;
                }
                match cursor.peek() {
                    None => FlashbackState::Complete,
                    Some("(") => {
                        cursor.consume("(")?;
                        FlashbackState::ReminderPermission
                    }
                    _ => {
                        return Err(cursor.fail(
                            "the canonical Flashback reminder or end of the keyword ability",
                        ));
                    }
                }
            }
            FlashbackState::ReminderPermission => {
                cursor.enter("keyword.flashback.reminder.permission");
                cursor.consume_phrase(&[
                    "you",
                    "may",
                    "cast",
                    "this",
                    "card",
                    "from",
                    "your",
                    "graveyard",
                    "for",
                    "its",
                    "flashback",
                    "cost",
                    ".",
                ])?;
                FlashbackState::ReminderExile
            }
            FlashbackState::ReminderExile => {
                cursor.enter("keyword.flashback.reminder.exile");
                cursor.consume_phrase(&["then", "exile", "it", "."])?;
                FlashbackState::ReminderClose
            }
            FlashbackState::ReminderClose => {
                cursor.enter("keyword.flashback.reminder.close");
                cursor.consume(")")?;
                FlashbackState::Complete
            }
            FlashbackState::Complete => {
                cursor.enter("keyword.flashback.complete");
                cursor.finish()?;
                return Ok(FlashbackKeywordNode {
                    cost: cursor
                        .require_value(printed_mana, "the retained fixed Flashback mana cost")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "flashback_tests.rs"]
mod tests;
