use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::counted_card::recognize_card_count_noun;
use super::{
    NumericExpression, PlayerPredicate, TargetListNode, TargetSelector,
    recognize_numeric_expression, recognize_target_list,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MillCardsEffectNode {
    pub target: TargetListNode,
    pub count: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MillCardsState {
    Target,
    Action,
    Count,
    Cards,
    Complete,
}

/// Recognizes targeted card milling as an atomic effect. Target declaration,
/// numeric expression, and sentence termination remain independent children
/// or parent-owned concepts.
pub(in crate::oracle::canonical) fn recognize_mill_cards_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<MillCardsEffectNode> {
    let mut state = MillCardsState::Target;
    let mut target = None;
    let mut count = None;
    loop {
        state = match state {
            MillCardsState::Target => {
                cursor.enter("effect.mill-cards.target");
                let parsed = recognize_target_list(cursor)?;
                cursor.require(
                    matches!(
                        parsed.slots.as_slice(),
                        [slot]
                            if slot.selector == TargetSelector::Player(PlayerPredicate::Any)
                                && slot.cardinality.minimum == 1
                                && slot.cardinality.maximum == 1
                    ),
                    "exactly one target player",
                )?;
                target = Some(parsed);
                MillCardsState::Action
            }
            MillCardsState::Action => {
                cursor.enter("effect.mill-cards.action");
                cursor.consume("mills")?;
                MillCardsState::Count
            }
            MillCardsState::Count => {
                cursor.enter("effect.mill-cards.count");
                count = Some(recognize_numeric_expression(cursor, None)?);
                MillCardsState::Cards
            }
            MillCardsState::Cards => {
                cursor.enter("effect.mill-cards.cards");
                recognize_card_count_noun(
                    cursor,
                    count
                        .as_ref()
                        .ok_or_else(|| cursor.fail("a populated mill count"))?,
                )?;
                MillCardsState::Complete
            }
            MillCardsState::Complete => {
                cursor.enter("effect.mill-cards.complete");
                return Ok(MillCardsEffectNode {
                    target: cursor.require_value(target, "a target player")?,
                    count: cursor.require_value(count, "a mill count")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "mill_tests.rs"]
mod tests;
