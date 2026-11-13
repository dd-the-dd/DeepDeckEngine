use super::super::{MachineResult, OracleTokenCursor};
use super::{BoundPermanentSetNode, ObjectPredicate, recognize_object_query};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AnaphoricObjectSetReferenceNode {
    pub antecedent: BoundPermanentSetNode,
    pub asserted_predicate: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DestroyReferencedSetEffectNode {
    pub objects: AnaphoricObjectSetReferenceNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectSetAnaphoraState {
    Demonstrative,
    ObjectKind,
    Validate,
    Complete,
}

fn predicate_contains(container: &ObjectPredicate, member: &ObjectPredicate) -> bool {
    container == member
        || matches!(container, ObjectPredicate::Intersection(operands) if operands.iter().any(|operand| predicate_contains(operand, member)))
}

/// Resolves `those OBJECT_KIND` against one prior bound set without knowing
/// which effect will consume the reference.
pub(in crate::oracle::canonical) fn recognize_anaphoric_object_set_reference(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&BoundPermanentSetNode>,
) -> MachineResult<AnaphoricObjectSetReferenceNode> {
    let mut state = ObjectSetAnaphoraState::Demonstrative;
    let mut asserted_predicate = None;
    loop {
        state = match state {
            ObjectSetAnaphoraState::Demonstrative => {
                cursor.enter("reference.object-set.demonstrative");
                cursor.consume("those")?;
                ObjectSetAnaphoraState::ObjectKind
            }
            ObjectSetAnaphoraState::ObjectKind => {
                cursor.enter("reference.object-set.object-kind");
                let query = recognize_object_query(cursor)?;
                cursor.require(
                    query.controller.is_none(),
                    "an unscoped object-set reference",
                )?;
                asserted_predicate = Some(query.predicate);
                ObjectSetAnaphoraState::Validate
            }
            ObjectSetAnaphoraState::Validate => {
                cursor.enter("reference.object-set.validation");
                let antecedent = antecedent
                    .ok_or_else(|| cursor.fail("a prior bound permanent set for `those`"))?;
                let asserted = asserted_predicate
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated object-kind assertion"))?;
                cursor.require(
                    predicate_contains(&antecedent.predicate, asserted),
                    "an object kind contained by the prior bound set",
                )?;
                ObjectSetAnaphoraState::Complete
            }
            ObjectSetAnaphoraState::Complete => {
                cursor.enter("reference.object-set.complete");
                return Ok(AnaphoricObjectSetReferenceNode {
                    antecedent: antecedent
                        .cloned()
                        .ok_or_else(|| cursor.fail("a prior bound permanent set"))?,
                    asserted_predicate: cursor
                        .require_value(asserted_predicate, "an asserted object kind")?,
                });
            }
        };
    }
}

/// Recognizes the atomic destruction effect whose object expression is a
/// previously bound set reference. The shared sequence owns the sentence
/// boundary and passes the antecedent explicitly.
pub(in crate::oracle::canonical) fn recognize_destroy_referenced_set_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<&BoundPermanentSetNode>,
) -> MachineResult<DestroyReferencedSetEffectNode> {
    cursor.enter("effect.destroy-referenced-set.action");
    cursor.consume("destroy")?;
    cursor.enter("effect.destroy-referenced-set.reference");
    Ok(DestroyReferencedSetEffectNode {
        objects: recognize_anaphoric_object_set_reference(cursor, antecedent)?,
    })
}
