//! Finite type-domain solver, not an Oracle parser or an activation planner.
//! The evaluator must describe one immutable state and preserve unsupported errors.
use super::mana_projection_contract::{
    ManaProjectionUnsupported, ManaType, ManaTypeProjection, NoManaReason,
};
use super::*;

pub(super) type ManaTypeTable = BTreeMap<String, ManaTypeProjection>;

#[derive(Debug)]
pub(super) enum ManaFixedPointFailure<E = ManaProjectionUnsupported> {
    Producer(E),
    DuplicateSource(String),
    MissingSource(String),
    NonMonotoneProducer(String),
    DomainSizeOverflow,
    DidNotConverge,
}

fn types(value: &ManaTypeProjection) -> BTreeSet<ManaType> {
    match value {
        ManaTypeProjection::NoMana(_) => BTreeSet::new(),
        ManaTypeProjection::Types(types) => types.clone(),
    }
}

/// A missing source is not the semantic empty set. This helper prevents query
/// implementations from accidentally using get(...).unwrap_or_default().
pub(super) fn require_source<'a>(
    table: &'a ManaTypeTable,
    source_id: &str,
) -> Result<&'a ManaTypeProjection, ManaFixedPointFailure> {
    table
        .get(source_id)
        .ok_or_else(|| ManaFixedPointFailure::MissingSource(source_id.to_string()))
}

/// Every successful strict iteration adds at least one of six type bits to a
/// source. The least fixed point therefore needs at most 6*N growth steps plus
/// one equality check. Empty ungrounded cycles are defined semantic NoMana.
/// No memoized DFS result can freeze an intermediate empty branch prematurely.
pub(super) fn solve_mana_type_fixed_point(
    source_ids: &[String],
    evaluate: impl FnMut(&str, &ManaTypeTable) -> Result<ManaTypeProjection, ManaProjectionUnsupported>,
) -> Result<ManaTypeTable, ManaFixedPointFailure> {
    solve_mana_type_fixed_point_with_error(source_ids, evaluate)
}

pub(super) fn solve_mana_type_fixed_point_with_error<E>(
    source_ids: &[String],
    mut evaluate: impl FnMut(&str, &ManaTypeTable) -> Result<ManaTypeProjection, E>,
) -> Result<ManaTypeTable, ManaFixedPointFailure<E>> {
    let mut current = ManaTypeTable::new();
    for id in source_ids {
        if current
            .insert(
                id.clone(),
                ManaTypeProjection::NoMana(NoManaReason::UndefinedType),
            )
            .is_some()
        {
            return Err(ManaFixedPointFailure::DuplicateSource(id.clone()));
        }
    }
    let iterations = source_ids
        .len()
        .checked_mul(6)
        .and_then(|n| n.checked_add(1))
        .ok_or(ManaFixedPointFailure::DomainSizeOverflow)?;
    for _ in 0..iterations {
        let mut next = ManaTypeTable::new();
        let mut changed = false;
        for id in source_ids {
            let value = evaluate(id, &current).map_err(ManaFixedPointFailure::Producer)?;
            let old_types = types(
                current
                    .get(id)
                    .ok_or_else(|| ManaFixedPointFailure::<E>::MissingSource(id.clone()))?,
            );
            let new_types = types(&value);
            if !old_types.is_subset(&new_types) {
                return Err(ManaFixedPointFailure::NonMonotoneProducer(id.clone()));
            }
            changed |= old_types != new_types;
            next.insert(id.clone(), value);
        }
        if !changed {
            return Ok(next);
        }
        current = next;
    }
    Err(ManaFixedPointFailure::DidNotConverge)
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_projection_fixed_point_tests.rs"]
mod tests;
