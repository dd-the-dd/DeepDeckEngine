use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DelayedStepEffectNode, TargetBinding, recognize_delayed_step_effect,
    recognize_referenced_draw_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DelayedDrawNode {
    pub delayed: DelayedStepEffectNode,
}

pub(in crate::oracle::canonical) fn recognize_delayed_draw(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<DelayedDrawNode> {
    cursor.enter("effect.delayed-draw.child");
    let delayed = recognize_delayed_step_effect(cursor, |cursor| {
        recognize_referenced_draw_effect(cursor, antecedent)
    })?;
    cursor.enter("effect.delayed-draw.complete");
    Ok(DelayedDrawNode { delayed })
}
