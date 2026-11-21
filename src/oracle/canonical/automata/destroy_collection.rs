use super::super::{MachineResult, OracleTokenCursor};
use super::{
    DestroyPermanentCollectionEffectNode, ObjectKind, recognize_object_collection,
    recognize_object_query_before_separator_with_source, recognize_optional_source_exclusion,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DestroyCollectionState {
    Action,
    Quantifier,
    ObjectCollection,
    PermanentDomain,
    Complete,
}

/// Recognizes a non-targeting collection destruction as one ordinary local
/// action. Punctuation and effect sequencing remain owned by EffectSequence.
pub(in crate::oracle::canonical) fn recognize_destroy_permanent_collection_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DestroyPermanentCollectionEffectNode> {
    recognize_destroy_permanent_collection_effect_with_source(cursor, false)
}

/// The same action where the parent proves its source (or its last-known
/// information) for numeric reads: a single-branch collection may compare a
/// property with the source's counters (`with mana value equal to the number
/// of charge counters on this artifact`).
pub(in crate::oracle::canonical) fn recognize_destroy_permanent_collection_effect_with_source(
    cursor: &mut OracleTokenCursor<'_>,
    source_numeric_reads: bool,
) -> MachineResult<DestroyPermanentCollectionEffectNode> {
    let mut state = DestroyCollectionState::Action;
    let mut objects = None;
    let mut exclude_source = false;
    loop {
        state = match state {
            DestroyCollectionState::Action => {
                cursor.enter("destroy.collection.action");
                cursor.consume("destroy")?;
                DestroyCollectionState::Quantifier
            }
            DestroyCollectionState::Quantifier => {
                cursor.enter("destroy.collection.quantifier");
                if cursor
                    .peek()
                    .is_some_and(|token| token.eq_ignore_ascii_case("all"))
                {
                    cursor.consume("all")?;
                } else {
                    cursor.consume("each")?;
                }
                DestroyCollectionState::ObjectCollection
            }
            DestroyCollectionState::ObjectCollection => {
                cursor.enter("destroy.collection.objects");
                exclude_source = recognize_optional_source_exclusion(cursor)?;
                objects = Some(match cursor.attempt(recognize_object_collection) {
                    Ok(objects) => objects,
                    Err(_) if source_numeric_reads => {
                        cursor.enter("destroy.collection.source-numeric-objects");
                        recognize_object_query_before_separator_with_source(cursor, true)?
                    }
                    Err(failure) => return Err(failure),
                });
                DestroyCollectionState::PermanentDomain
            }
            DestroyCollectionState::PermanentDomain => {
                cursor.enter("destroy.collection.permanent-domain");
                let query = objects
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated object collection"))?;
                cursor.require(
                    query.predicate.supports_domain(ObjectKind::Permanent)
                        && query.predicate.has_domain(ObjectKind::Permanent),
                    "a collection explicitly anchored in the battlefield-permanent domain",
                )?;
                DestroyCollectionState::Complete
            }
            DestroyCollectionState::Complete => {
                cursor.enter("destroy.collection.complete");
                return Ok(DestroyPermanentCollectionEffectNode {
                    objects: cursor.require_value(objects, "a permanent object collection")?,
                    exclude_source,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "destroy_collection_tests.rs"]
mod tests;
