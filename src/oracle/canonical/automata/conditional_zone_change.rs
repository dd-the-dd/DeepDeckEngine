use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ConditionalZoneChangeNode, LocalEffectNode, TargetBinding,
    recognize_bound_object_numeric_condition, recognize_zone_change_effect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConditionalZoneChangeState {
    ZoneChange,
    Condition,
    Terminator,
    Complete,
}

/// Composes an atomic targeted zone change with a resolution-time condition
/// over the same target. The target is declared by the first child; the
/// condition receives only its typed binding and cannot invent a new target.
pub(in crate::oracle::canonical) fn recognize_conditional_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionalZoneChangeNode> {
    let mut state = ConditionalZoneChangeState::ZoneChange;
    let mut zone_change = None;
    let mut condition = None;
    loop {
        state = match state {
            ConditionalZoneChangeState::ZoneChange => {
                cursor.enter("conditional-zone-change.effect");
                let effect = recognize_zone_change_effect(cursor)?;
                let LocalEffectNode::ZoneChange(effect) = effect else {
                    return Err(cursor.fail("a targeted zone-change effect"));
                };
                zone_change = Some(effect);
                ConditionalZoneChangeState::Condition
            }
            ConditionalZoneChangeState::Condition => {
                cursor.enter("conditional-zone-change.condition");
                condition = Some(recognize_bound_object_numeric_condition(
                    cursor,
                    TargetBinding {
                        effect_index: 0,
                        slot_index: 0,
                    },
                )?);
                ConditionalZoneChangeState::Terminator
            }
            ConditionalZoneChangeState::Terminator => {
                cursor.enter("conditional-zone-change.terminator");
                cursor.consume(".")?;
                ConditionalZoneChangeState::Complete
            }
            ConditionalZoneChangeState::Complete => {
                cursor.enter("conditional-zone-change.complete");
                cursor.finish()?;
                return Ok(ConditionalZoneChangeNode {
                    zone_change: cursor
                        .require_value(zone_change, "a populated targeted zone change")?,
                    condition: cursor
                        .require_value(condition, "a condition bound to the zone-change target")?,
                });
            }
        };
    }
}
