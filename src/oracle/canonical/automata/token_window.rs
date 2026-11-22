//! A child parser that reads a window `[start, end)` of an ability's token
//! stream sees window-local token positions. Name links, name-resolution
//! failures and machine failures carry token positions, so they move between
//! the ability and the window together: spans stay byte offsets in the
//! original Oracle text and never change.
use crate::oracle::syntax::{
    NameResolutionFailure, NameResolverMemory, ResolvedNameLink, UnresolvedName,
};

use super::super::MachineFailure;

/// A resolved name link entirely inside the window, in window positions.
pub(in crate::oracle::canonical) fn rebase_name_link(
    link: &ResolvedNameLink,
    start: usize,
    end: usize,
) -> Option<ResolvedNameLink> {
    (link.token_start >= start && link.token_end <= end).then(|| ResolvedNameLink {
        kind: link.kind,
        canonical_name: link.canonical_name.clone(),
        raw: link.raw.clone(),
        token_start: link.token_start - start,
        token_end: link.token_end - start,
        span_start: link.span_start,
        span_end: link.span_end,
    })
}

fn rebase_unresolved_name(
    unresolved: &UnresolvedName,
    start: usize,
    end: usize,
) -> Option<UnresolvedName> {
    (unresolved.token_start >= start && unresolved.token_end <= end).then(|| UnresolvedName {
        raw: unresolved.raw.clone(),
        token_start: unresolved.token_start - start,
        token_end: unresolved.token_end - start,
        span_start: unresolved.span_start,
        span_end: unresolved.span_end,
    })
}

fn rebase_name_memory(memory: &mut NameResolverMemory, start: usize, end: usize) {
    memory.position = memory.position.saturating_sub(start).min(end - start);
    memory.checkpoint = memory.checkpoint.saturating_sub(start).min(end - start);
    memory.best_link = memory
        .best_link
        .as_ref()
        .and_then(|link| rebase_name_link(link, start, end));
}

/// The name-resolution failure restricted to the window. A window without an
/// unresolved name of its own has no name failure.
pub(in crate::oracle::canonical) fn rebase_name_failure(
    failure: &NameResolutionFailure,
    start: usize,
    end: usize,
) -> Option<NameResolutionFailure> {
    let unresolved = failure
        .unresolved
        .iter()
        .filter_map(|name| rebase_unresolved_name(name, start, end))
        .collect::<Vec<_>>();
    if unresolved.is_empty() {
        return None;
    }
    let mut rebased = failure.clone();
    rebased.resolved = failure
        .resolved
        .iter()
        .filter_map(|link| rebase_name_link(link, start, end))
        .collect();
    rebased.unresolved = unresolved;
    rebase_name_memory(&mut rebased.memory, start, end);
    for transition in &mut rebased.trace {
        rebase_name_memory(&mut transition.memory, start, end);
    }
    Some(rebased)
}

/// A window failure reported at its position in the enclosing token stream.
pub(in crate::oracle::canonical) fn rebase_machine_failure(
    mut failure: MachineFailure,
    start: usize,
) -> MachineFailure {
    failure.token_index += start;
    for transition in &mut failure.trace {
        transition.token_index += start;
    }
    failure
}
