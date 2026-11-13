use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ActivationCostSequenceNode, TargetCardZoneChangeEffectNode, recognize_activation_cost_sequence,
    recognize_target_card_zone_change_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivatedCardZoneChangeNode {
    pub costs: ActivationCostSequenceNode,
    pub effect: TargetCardZoneChangeEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivatedCardZoneChangeState {
    Costs,
    EffectSeparator,
    Effect,
    Terminator,
    Complete,
}

#[derive(Default)]
struct ActivatedCardZoneChangeFrame {
    costs: Option<ActivationCostSequenceNode>,
    effect: Option<TargetCardZoneChangeEffectNode>,
}

/// Recognizes an activated, targeted card move whose source is a zone rather
/// than the battlefield. Cost recognition is delegated to the shared
/// activation-cost automaton; this machine owns only the effect envelope and
/// its card-zone relation.
pub(in crate::oracle::canonical) fn recognize_activated_card_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivatedCardZoneChangeNode> {
    let mut state = ActivatedCardZoneChangeState::Costs;
    let mut frame = ActivatedCardZoneChangeFrame::default();
    loop {
        state = match state {
            ActivatedCardZoneChangeState::Costs => {
                cursor.enter("activation.card-zone-change.costs");
                frame.costs = Some(recognize_activation_cost_sequence(cursor)?);
                ActivatedCardZoneChangeState::EffectSeparator
            }
            ActivatedCardZoneChangeState::EffectSeparator => {
                cursor.enter("activation.card-zone-change.effect-separator");
                cursor.consume(":")?;
                ActivatedCardZoneChangeState::Effect
            }
            ActivatedCardZoneChangeState::Effect => {
                cursor.enter("effect.activated-card-zone-change.effect");
                frame.effect = Some(recognize_target_card_zone_change_effect(cursor)?);
                ActivatedCardZoneChangeState::Terminator
            }
            ActivatedCardZoneChangeState::Terminator => {
                cursor.enter("effect.activated-card-zone-change.terminator");
                cursor.consume(".")?;
                ActivatedCardZoneChangeState::Complete
            }
            ActivatedCardZoneChangeState::Complete => {
                cursor.enter("effect.activated-card-zone-change.complete");
                cursor.finish()?;
                return Ok(ActivatedCardZoneChangeNode {
                    costs: cursor.require_value(frame.costs, "activation costs")?,
                    effect: cursor.require_value(frame.effect, "a targeted card-zone effect")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "activated_zone_change_tests.rs"]
mod tests;
