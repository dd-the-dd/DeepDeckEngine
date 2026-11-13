use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::recognize_sacrifice_action;
use super::{
    ObjectControllerScope, ObjectKind, ObjectQueryNode, recognize_object_query_before_separator,
};

/// One chosen permanent paid as an activation cost. The cost parent assigns
/// its stable ordinal; the shared query retains every characteristic filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SelectedSacrificeCostNode {
    pub object: ObjectQueryNode,
    pub cost_index: usize,
}

pub(in crate::oracle::canonical) fn recognize_selected_sacrifice_cost(
    cursor: &mut OracleTokenCursor<'_>,
    cost_index: usize,
) -> MachineResult<SelectedSacrificeCostNode> {
    let action = recognize_sacrifice_action(cursor, |cursor| {
        cursor.enter("cost.selected-sacrifice.article");
        let article = cursor
            .peek()
            .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
            .ok_or_else(|| cursor.fail("`a` or `an` before one chosen permanent"))?
            .to_string();
        cursor.consume(&article)?;
        cursor.enter("cost.selected-sacrifice.object-query");
        recognize_object_query_before_separator(cursor)
    })?;
    cursor.enter("cost.selected-sacrifice.controller");
    cursor.require(
        matches!(
            action.operand.controller,
            None | Some(ObjectControllerScope::AbilityController)
        ),
        "a permanent controlled by the activation's payer",
    )?;
    cursor.enter("cost.selected-sacrifice.permanent-domain");
    cursor.require(
        action
            .operand
            .predicate
            .supports_domain(ObjectKind::Permanent)
            && action.operand.predicate.has_nominal_head(),
        "a battlefield-permanent predicate with a nominal head",
    )?;
    cursor.enter("cost.selected-sacrifice.complete");
    Ok(SelectedSacrificeCostNode {
        object: action.operand,
        cost_index,
    })
}

#[cfg(test)]
#[path = "selected_sacrifice_cost_tests.rs"]
mod tests;
