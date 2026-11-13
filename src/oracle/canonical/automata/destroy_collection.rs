use super::super::{MachineResult, OracleTokenCursor};
use super::{DestroyPermanentCollectionEffectNode, ObjectKind, recognize_object_collection};

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
    let mut state = DestroyCollectionState::Action;
    let mut objects = None;
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
                objects = Some(recognize_object_collection(cursor)?);
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
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "destroy_collection_tests.rs"]
mod tests;
