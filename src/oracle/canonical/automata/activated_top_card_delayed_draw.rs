use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, DelayedStepEffectNode, LocalEffectNode, TopCardsEffectNode,
    recognize_activation_cost_sequence, recognize_delayed_step_effect, recognize_draw_cards_effect,
    recognize_top_cards_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedTopCardDelayedDrawNode {
    pub costs: ActivationCostSequenceNode,
    pub inspection: TopCardsEffectNode,
    pub delayed: DelayedStepEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivatedTopCardDelayedDrawState {
    Costs,
    EffectSeparator,
    Inspection,
    InspectionBoundary,
    DelayedDraw,
    Terminator,
    Complete,
}

/// Composes existing cost, top-card inspection, draw, and delayed-step
/// children. This parent owns only their activated-ability sentence order.
pub(in crate::oracle::canonical) fn recognize_activated_top_card_delayed_draw(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivatedTopCardDelayedDrawNode> {
    let mut state = ActivatedTopCardDelayedDrawState::Costs;
    let mut costs = None;
    let mut inspection = None;
    let mut delayed = None;
    loop {
        state = match state {
            ActivatedTopCardDelayedDrawState::Costs => {
                cursor.enter("activation.top-card-delayed-draw.costs");
                costs = Some(recognize_activation_cost_sequence(cursor)?);
                ActivatedTopCardDelayedDrawState::EffectSeparator
            }
            ActivatedTopCardDelayedDrawState::EffectSeparator => {
                cursor.enter("activation.top-card-delayed-draw.effect-separator");
                cursor.consume(":")?;
                ActivatedTopCardDelayedDrawState::Inspection
            }
            ActivatedTopCardDelayedDrawState::Inspection => {
                cursor.enter("activation.top-card-delayed-draw.inspection");
                inspection = Some(recognize_top_cards_effect(cursor)?);
                ActivatedTopCardDelayedDrawState::InspectionBoundary
            }
            ActivatedTopCardDelayedDrawState::InspectionBoundary => {
                cursor.enter("activation.top-card-delayed-draw.inspection-boundary");
                cursor.consume(".")?;
                ActivatedTopCardDelayedDrawState::DelayedDraw
            }
            ActivatedTopCardDelayedDrawState::DelayedDraw => {
                cursor.enter("activation.top-card-delayed-draw.delayed-draw");
                delayed = Some(recognize_delayed_step_effect(cursor, |cursor| {
                    recognize_draw_cards_effect(cursor).map(LocalEffectNode::DrawCards)
                })?);
                ActivatedTopCardDelayedDrawState::Terminator
            }
            ActivatedTopCardDelayedDrawState::Terminator => {
                cursor.enter("activation.top-card-delayed-draw.terminator");
                cursor.consume(".")?;
                ActivatedTopCardDelayedDrawState::Complete
            }
            ActivatedTopCardDelayedDrawState::Complete => {
                cursor.enter("activation.top-card-delayed-draw.complete");
                cursor.finish()?;
                return Ok(ActivatedTopCardDelayedDrawNode {
                    costs: cursor.require_value(costs, "a populated activation-cost sequence")?,
                    inspection: cursor
                        .require_value(inspection, "a populated top-card inspection")?,
                    delayed: cursor.require_value(delayed, "a populated delayed draw effect")?,
                });
            }
        };
    }
}
