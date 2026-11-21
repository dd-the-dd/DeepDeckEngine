use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, recognize_source_reference_with_antecedent};

#[path = "atomic_action.rs"]
pub(super) mod atomic_action;
use atomic_action::recognize_sacrifice_action;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SacrificeSourceEffectNode {
    pub source: LocalObjectReference,
}

/// Recognizes the independent atomic effect `sacrifice SOURCE.`. Current-card
/// names, typed `this` references, and a bound source anaphor all use the
/// shared source resolver.
pub(in crate::oracle::canonical) fn recognize_sacrifice_source_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<&LocalObjectReference>,
) -> MachineResult<SacrificeSourceEffectNode> {
    let action = recognize_sacrifice_action(cursor, |cursor| {
        cursor.enter("effect.sacrifice-source.source");
        recognize_source_reference_with_antecedent(cursor, name_links, name_failure, antecedent)
    })?;
    Ok(SacrificeSourceEffectNode {
        source: action.operand,
    })
}

#[cfg(test)]
#[path = "source_sacrifice_tests.rs"]
mod tests;
