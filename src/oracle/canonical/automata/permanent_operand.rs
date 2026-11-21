use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, ObjectKind, ObjectQueryNode, recognize_object_collection,
    recognize_source_reference_with_antecedent,
};

/// A location in a reusable action is independent of the verb using it.
/// Collection quantification does not imply a target or a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentOperandNode {
    Targets(super::TargetListNode),
    BoundReference(super::PermanentEffectReference),
    Reference(LocalObjectReference),
    Collection(ObjectQueryNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PermanentOperandState {
    Head,
    Reference,
    Collection,
    Domain,
    Complete,
}

#[derive(Default)]
struct PermanentOperandFrame {
    reference: Option<LocalObjectReference>,
    collection: Option<ObjectQueryNode>,
}

/// The caller owns the action, separator and following effects. This child
/// preserves the same ObjectQuery coordination/controller boundaries used by
/// destruction, static collections and conditions.
pub(in crate::oracle::canonical) fn recognize_permanent_operand(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<&LocalObjectReference>,
) -> MachineResult<PermanentOperandNode> {
    let mut state = PermanentOperandState::Head;
    let mut frame = PermanentOperandFrame::default();
    loop {
        state = match state {
            PermanentOperandState::Head => {
                cursor.enter("permanent.operand.head");
                if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("each") || raw.eq_ignore_ascii_case("all")
                }) {
                    let quantifier = cursor.peek().expect("checked quantifier").to_string();
                    cursor.consume(&quantifier)?;
                    PermanentOperandState::Collection
                } else {
                    PermanentOperandState::Reference
                }
            }
            PermanentOperandState::Reference => {
                frame.reference = Some(recognize_source_reference_with_antecedent(
                    cursor,
                    name_links,
                    name_failure,
                    antecedent,
                )?);
                PermanentOperandState::Complete
            }
            PermanentOperandState::Collection => {
                cursor.enter("permanent.operand.collection");
                frame.collection = Some(recognize_object_collection(cursor)?);
                PermanentOperandState::Domain
            }
            PermanentOperandState::Domain => {
                cursor.enter("permanent.operand.collection.domain");
                let query = frame
                    .collection
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated permanent collection"))?;
                cursor.require(
                    query.predicate.has_domain(ObjectKind::Permanent)
                        && query.predicate.supports_domain(ObjectKind::Permanent),
                    "an explicitly battlefield-permanent collection, not players or cards",
                )?;
                PermanentOperandState::Complete
            }
            PermanentOperandState::Complete => {
                cursor.enter("permanent.operand.complete");
                if let Some(reference) = frame.reference {
                    return Ok(PermanentOperandNode::Reference(reference));
                }
                return Ok(PermanentOperandNode::Collection(
                    cursor.require_value(frame.collection, "a typed permanent operand")?,
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "permanent_operand_tests.rs"]
mod tests;
