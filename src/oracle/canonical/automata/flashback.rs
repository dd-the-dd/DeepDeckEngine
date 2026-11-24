//! A keyword envelope around the shared typed cost child, never a spell effect.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, LocalActivationCostNode, ManaCostNode,
    recognize_activation_cost_sequence,
};

/// `Flashback [cost]` (CR 702.34a). A mana cost follows the keyword; any
/// other payment is the shared cost sequence after an em dash
/// (`Flashback—{1}{U}, Pay 3 life.`), which the keyword-cost lowering reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct FlashbackKeywordNode {
    pub costs: ActivationCostSequenceNode,
}

impl FlashbackKeywordNode {
    /// The printed cost when it is exactly one mana payment.
    pub(in crate::oracle::canonical) fn mana(&self) -> Option<&ManaCostNode> {
        match self.costs.costs.as_slice() {
            [LocalActivationCostNode::PayMana(mana)] => Some(mana),
            _ => None,
        }
    }
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
    let mut dashed = false;
    let mut retained = None;
    loop {
        state = match state {
            FlashbackState::Keyword => {
                cursor.enter("keyword.flashback.keyword");
                cursor.consume("flashback")?;
                if cursor.peek() == Some("\u{2014}") {
                    cursor.consume("\u{2014}")?;
                    dashed = true;
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
                // Without the em dash only one mana payment follows the
                // keyword; a nonmana or composite cost is written after it.
                if !dashed {
                    let [LocalActivationCostNode::PayMana(_)] = costs.costs.as_slice() else {
                        return Err(cursor.fail(
                            "one mana payment, or a nonmana or composite cost after the Flashback em dash",
                        ));
                    };
                }
                for cost in &costs.costs {
                    if let LocalActivationCostNode::PayMana(mana) = cost {
                        mana.fixed_runtime_mana_value()
                            .map_err(|error| cursor.fail(error.expected()))?;
                    }
                }
                retained = Some(costs);
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
                ])?;
                // Newer printings restate CR 702.34a's additional costs.
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                {
                    cursor.consume_phrase(&["and", "any", "additional", "costs"])?;
                }
                cursor.consume(".")?;
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
                    costs: cursor.require_value(retained, "the retained Flashback cost")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "flashback_tests.rs"]
mod tests;
