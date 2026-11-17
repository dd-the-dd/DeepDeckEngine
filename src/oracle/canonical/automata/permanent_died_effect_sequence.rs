use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectSequenceNode, PermanentDiedEventNode, recognize_effect_sequence,
    recognize_permanent_died_event,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentDiedEffectSequenceNode {
    pub event: PermanentDiedEventNode,
    pub effects: EffectSequenceNode,
}

pub(in crate::oracle::canonical) fn recognize_permanent_died_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<PermanentDiedEffectSequenceNode> {
    cursor.enter("trigger.permanent-died.trigger");
    cursor.consume("whenever")?;
    let event = recognize_permanent_died_event(cursor, name_links, name_failure)?;
    cursor.enter("trigger.permanent-died.separator");
    cursor.consume(",")?;
    let effects = recognize_effect_sequence(cursor, name_links, name_failure)?;
    cursor.enter("trigger.permanent-died.complete");
    Ok(PermanentDiedEffectSequenceNode { event, effects })
}

#[cfg(test)]
#[path = "permanent_died_effect_sequence_tests.rs"]
mod tests;
