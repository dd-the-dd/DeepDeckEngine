use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, ObjectControllerScope, ObjectPredicate, recognize_object_query,
    recognize_source_reference, source_reference_starts_at,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentDiedEventBranchNode {
    Source(LocalObjectReference),
    Filtered {
        predicate: ObjectPredicate,
        controller: Option<ObjectControllerScope>,
        exclude_source: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentDiedEventNode {
    pub alternatives: Vec<PermanentDiedEventBranchNode>,
}

fn recognize_branch(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<PermanentDiedEventBranchNode> {
    cursor.enter("event.permanent-died.subject");
    if source_reference_starts_at(cursor.position(), cursor.peek(), name_links, name_failure) {
        return recognize_source_reference(cursor, name_links, name_failure)
            .map(PermanentDiedEventBranchNode::Source);
    }

    let exclude_source = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("another"));
    if exclude_source {
        cursor.consume("another")?;
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
    }
    cursor.enter("event.permanent-died.predicate");
    let query = recognize_object_query(cursor)?;
    cursor.require(
        query
            .predicate
            .supports_domain(super::ObjectKind::Permanent),
        "an event subject valid in the battlefield-permanent domain",
    )?;
    cursor.require(
        query.controller != Some(ObjectControllerScope::NotAbilityController),
        "an explicit controller or opponent-controller relation",
    )?;
    Ok(PermanentDiedEventBranchNode::Filtered {
        predicate: query.predicate,
        controller: query.controller,
        exclude_source,
    })
}

/// Recognizes the subject of a `dies` event as typed alternatives. A source
/// branch is retained independently of type, which preserves last-known source
/// identity even if the source has stopped being a creature when it dies.
pub(in crate::oracle::canonical) fn recognize_permanent_transition_subject(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<PermanentDiedEventNode> {
    let mut alternatives = vec![recognize_branch(cursor, name_links, name_failure)?];
    while cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
    {
        cursor.enter("event.permanent-died.union");
        cursor.consume("or")?;
        alternatives.push(recognize_branch(cursor, name_links, name_failure)?);
    }
    Ok(PermanentDiedEventNode { alternatives })
}

pub(in crate::oracle::canonical) fn recognize_permanent_died_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<PermanentDiedEventNode> {
    let event = recognize_permanent_transition_subject(cursor, name_links, name_failure)?;
    cursor.enter("event.permanent-died.action");
    cursor.consume("dies")?;
    cursor.enter("event.permanent-died.complete");
    Ok(event)
}

#[cfg(test)]
#[path = "permanent_died_tests.rs"]
mod tests;
