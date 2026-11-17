use super::super::{MachineResult, OracleTokenCursor};
use super::*;

pub(in crate::oracle::canonical) type StaticSpellReductionNode = CastingCostReductionNode;

/// Compatibility entry point, not another reduction grammar or amount node.
pub(in crate::oracle::canonical) fn recognize_static_spell_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticSpellReductionNode> {
    let node = recognize_casting_cost_reduction(cursor)?;
    cursor.require(
        matches!(&node.subject, CastingReductionSubject::SpellSet(_)),
        "a queried spell set for this static compatibility entry point",
    )?;
    Ok(node)
}

#[cfg(test)]
#[path = "static_spell_reduction_tests.rs"]
mod tests;
