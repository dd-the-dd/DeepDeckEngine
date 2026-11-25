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
    /// The registered nominal a `this NOUN` source branch was written with
    /// (`this creature or another creature you control`): what a later
    /// nominal anaphor of the event's object may follow from. A name or any
    /// other source surface writes none.
    pub source_nominal: Option<super::TargetNominal>,
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
    // The subject's own qualifiers may name the source (`a creature dealt
    // damage by NAME this turn`): its name links are installed for them.
    let previous = cursor.replace_source_names(Some(name_links.into()));
    let query = recognize_object_query(cursor);
    cursor.replace_source_names(previous);
    let query = query?;
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
    let mut source_nominal = None;
    let mut read_branch =
        |cursor: &mut OracleTokenCursor<'_>| -> MachineResult<PermanentDiedEventBranchNode> {
            let nominal = super::this_object_nominal_at(cursor);
            let branch = recognize_branch(cursor, name_links, name_failure)?;
            if matches!(branch, PermanentDiedEventBranchNode::Source(_)) {
                source_nominal = nominal;
            }
            Ok(branch)
        };
    let mut alternatives = vec![read_branch(cursor)?];
    while cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
    {
        cursor.enter("event.permanent-died.union");
        cursor.consume("or")?;
        alternatives.push(read_branch(cursor)?);
    }
    // `NAME or another NOUN`: a named source is of the kind `another`
    // presupposes (`binding_environment::nominal_presupposed_by_another`).
    if source_nominal.is_none() {
        source_nominal = super::presupposed_source_nominal(&alternatives);
    }
    Ok(PermanentDiedEventNode {
        alternatives,
        source_nominal,
    })
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
