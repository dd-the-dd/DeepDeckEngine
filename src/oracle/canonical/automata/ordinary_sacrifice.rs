//! Ordinary resolution action. Costs retain their separate payer contract.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SacrificeEffectNode {
    Source(SacrificeSourceEffectNode),
    Selected {
        object: ObjectQueryNode,
        count: NumericExpression,
    },
}

pub(in crate::oracle::canonical) fn recognize_ordinary_sacrifice_effect(
    cursor: &mut OracleTokenCursor<'_>,
    names: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<&LocalObjectReference>,
) -> MachineResult<SacrificeEffectNode> {
    // Probe only the operand's typed reference head; the original source
    // reader retains aliases, diagnostics, and its exact semantic node.
    let mut probe = cursor.clone();
    probe.consume("sacrifice")?;
    if source_or_anaphor_reference_starts_at(probe.position(), probe.peek(), names, name_failure) {
        return recognize_sacrifice_source_effect(cursor, names, name_failure, antecedent)
            .map(SacrificeEffectNode::Source);
    }
    let action = source_sacrifice::atomic_action::recognize_sacrifice_action(cursor, |cursor| {
        recognize_selected_sacrifice_operand(cursor, SacrificeOperandOwner::AbilityController)
    })?;
    Ok(action.operand)
}

/// Whose permanents the operand ranges over. The acting player of an ordinary
/// effect is the ability controller, who may restate it (`a creature you
/// control`). A quantified player acts on its own permanents, so any explicit
/// controller relation would contradict the iteration binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SacrificeOperandOwner {
    AbilityController,
    ActingPlayer,
}

/// Shared quantity plus object-query operand of a selected sacrifice.
pub(in crate::oracle::canonical) fn recognize_selected_sacrifice_operand(
    cursor: &mut OracleTokenCursor<'_>,
    owner: SacrificeOperandOwner,
) -> MachineResult<SacrificeEffectNode> {
    cursor.enter("effect.sacrifice.quantity");
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("effect.sacrifice.object-query");
    let object = recognize_object_query_with_shared_cardinality(cursor, &count)?;
    cursor.require(
        match owner {
            SacrificeOperandOwner::AbilityController => matches!(
                object.controller,
                None | Some(ObjectControllerScope::AbilityController)
            ),
            SacrificeOperandOwner::ActingPlayer => object.controller.is_none(),
        },
        match owner {
            SacrificeOperandOwner::AbilityController => {
                "a permanent controlled by the resolving ability's controller"
            }
            SacrificeOperandOwner::ActingPlayer => {
                "a permanent query without a controller relation (the acting player's own permanents)"
            }
        },
    )?;
    cursor.require(
        object.predicate.supports_domain(ObjectKind::Permanent)
            && object.predicate.has_nominal_head(),
        "a permanent query with a nominal head",
    )?;
    Ok(SacrificeEffectNode::Selected { object, count })
}

#[cfg(test)]
#[path = "ordinary_sacrifice_tests.rs"]
mod tests;
