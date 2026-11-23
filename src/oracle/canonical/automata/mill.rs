use super::super::{ChildOutcome, MachineResult, OracleTokenCursor};
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
    Player,
    Count,
    Cards,
    Complete,
}

/// Recognizes targeted card milling as an atomic effect. Target declaration,
/// numeric expression, and sentence termination remain independent children
/// or parent-owned concepts.
///
/// The frame commits only after its subject and its lemma (`mills`) are
/// read: `target opponent reveals ...` is not a mill clause, so it returns
/// `NoMatch` and never becomes the clause's diagnostic merely because the
/// target list read further than a sibling (roadmap S0).
pub(in crate::oracle::canonical) fn try_recognize_mill_cards_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> ChildOutcome<MillCardsEffectNode> {
    cursor.transaction(|cursor, commit| {
        let mut state = MillCardsState::Target;
        let mut target = None;
        let mut count = None;
        loop {
            state = match state {
                MillCardsState::Target => {
                    cursor.enter("effect.mill-cards.target");
                    target = Some(recognize_target_list(cursor)?);
                    MillCardsState::Action
                }
                MillCardsState::Action => {
                    cursor.enter("effect.mill-cards.action");
                    crate::oracle::canonical::automata::consume_lemma(
                        cursor,
                        crate::oracle::canonical::automata::VerbLemma::Mill,
                        crate::oracle::canonical::automata::VerbAgreement::ThirdPersonSingular,
                    )?;
                    // Subject and lemma read: this clause is a mill clause.
                    commit.commit();
                    MillCardsState::Player
                }
                MillCardsState::Player => {
                    cursor.enter("effect.mill-cards.player");
                    cursor.require(
                        target.as_ref().is_some_and(|parsed| {
                            matches!(
                                parsed.slots.as_slice(),
                                [slot]
                                    if slot.selector == TargetSelector::Player(PlayerPredicate::Any)
                                        && slot.cardinality.minimum == 1
                                        && slot.cardinality.maximum == 1
                            )
                        }),
                        "exactly one target player as the milling subject",
                    )?;
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
    })
}

/// The mill frame for a parent without a sibling reader at a `target` head.
/// A clause that is not a mill clause is reported by its deepest component
/// (`failure_owner::clause_head_failure`): the target subject was read and its
/// verb names the missing lemma, instead of `exactly one target player`.
pub(in crate::oracle::canonical) fn recognize_mill_cards_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<MillCardsEffectNode> {
    let start = cursor.position();
    try_recognize_mill_cards_effect(cursor).or_frame_failure(|_| {
        crate::oracle::canonical::failure_owner::clause_head_failure(cursor, start)
    })
}

#[cfg(test)]
#[path = "mill_tests.rs"]
mod tests;
