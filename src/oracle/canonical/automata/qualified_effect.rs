use super::super::{MachineResult, OracleTokenCursor};
use super::{
    GainLifeEffectNode, LocalEffectNode, LocalObjectReference, ZoneChangeEffectNode,
    recognize_life_effect, recognize_zone_change_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AdditionalCostPaidConditionNode {
    pub spell: LocalObjectReference,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AdditionalCostPaidFollowupNode {
    pub zone_change: ZoneChangeEffectNode,
    pub condition: AdditionalCostPaidConditionNode,
    pub then_effect: GainLifeEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QualifiedFollowupState {
    ZoneChange,
    SentenceBoundary,
    Condition,
    Followup,
    Terminator,
    Complete,
}

#[derive(Default)]
struct QualifiedFollowupFrame {
    zone_change: Option<ZoneChangeEffectNode>,
    condition: Option<AdditionalCostPaidConditionNode>,
    then_effect: Option<GainLifeEffectNode>,
}

/// Composes a targeted zone change, a mana-value target qualifier, and an
/// independent effect guarded by payment of the spell's additional cost.
pub(in crate::oracle::canonical) fn recognize_additional_cost_paid_followup(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AdditionalCostPaidFollowupNode> {
    let mut state = QualifiedFollowupState::ZoneChange;
    let mut frame = QualifiedFollowupFrame::default();
    loop {
        state = match state {
            QualifiedFollowupState::ZoneChange => {
                cursor.enter("sequence.qualified-zone-change.effect");
                let effect = recognize_zone_change_effect(cursor)?;
                let LocalEffectNode::ZoneChange(zone_change) = effect else {
                    return Err(cursor.fail("a targeted zone-change effect"));
                };
                frame.zone_change = Some(zone_change);
                QualifiedFollowupState::SentenceBoundary
            }
            QualifiedFollowupState::SentenceBoundary => {
                cursor.enter("sequence.qualified-zone-change.sentence-boundary");
                cursor.consume(".")?;
                QualifiedFollowupState::Condition
            }
            QualifiedFollowupState::Condition => {
                cursor.enter("condition.additional-cost-paid.if");
                cursor.consume_phrase(&[
                    "if",
                    "this",
                    "spell's",
                    "additional",
                    "cost",
                    "was",
                    "paid",
                    ",",
                ])?;
                frame.condition = Some(AdditionalCostPaidConditionNode {
                    spell: LocalObjectReference::SourceSelf,
                });
                QualifiedFollowupState::Followup
            }
            QualifiedFollowupState::Followup => {
                cursor.enter("condition.additional-cost-paid.then-effect");
                let effect = recognize_life_effect(cursor, None)?;
                let LocalEffectNode::GainLife(gain_life) = effect else {
                    return Err(cursor.fail("a life-gain followup effect"));
                };
                frame.then_effect = Some(gain_life);
                QualifiedFollowupState::Terminator
            }
            QualifiedFollowupState::Terminator => {
                cursor.enter("condition.additional-cost-paid.terminator");
                cursor.consume(".")?;
                QualifiedFollowupState::Complete
            }
            QualifiedFollowupState::Complete => {
                cursor.enter("sequence.qualified-zone-change.complete");
                cursor.finish()?;
                let zone_change = cursor
                    .require_value(frame.zone_change, "a populated targeted zone-change effect")?;
                return Ok(AdditionalCostPaidFollowupNode {
                    zone_change,
                    condition: cursor.require_value(
                        frame.condition,
                        "a populated additional-cost-paid condition",
                    )?,
                    then_effect: cursor.require_value(
                        frame.then_effect,
                        "a populated conditional followup effect",
                    )?,
                });
            }
        };
    }
}
