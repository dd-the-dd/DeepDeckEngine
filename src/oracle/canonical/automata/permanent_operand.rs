use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, ObjectKind, ObjectNounPhrase, ObjectNounPhraseExpectation,
    ObjectQueryNode, recognize_object_noun_phrase, recognize_source_reference_with_antecedent,
};

/// A location in a reusable action is independent of the verb using it.
/// Collection quantification does not imply a target or a choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentOperandNode {
    Targets(super::TargetListNode),
    BoundReference(super::PermanentEffectReference),
    Reference(LocalObjectReference),
    /// `each [other] QUERY`: universal membership; `other` excludes the
    /// bound ability source, never objects merely sharing its name.
    Collection {
        objects: ObjectQueryNode,
        exclude_source: bool,
    },
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
    phrase: Option<ObjectNounPhrase>,
    collection: Option<ObjectQueryNode>,
    exclude_source: bool,
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
                // `each [other] QUERY`, `all QUERY`: the shared object noun
                // phrase reads the universal determiner, the exclusion, the
                // coordinated query and its domain; this parent type-checks.
                let phrase = recognize_object_noun_phrase(
                    cursor,
                    ObjectNounPhraseExpectation::in_domains(&[ObjectKind::Permanent]),
                )?;
                cursor.require(
                    phrase.is_universal(),
                    "a universal permanent collection after `each` or `all`",
                )?;
                let mut collection = phrase.query.clone().into_object_query(cursor)?;
                super::refuse_other_than_relation_anchor(
                    cursor,
                    &collection.predicate,
                    phrase.exclude_source,
                )?;
                collection.exclude_source = phrase.exclude_source;
                frame.exclude_source = phrase.exclude_source;
                frame.collection = Some(collection);
                frame.phrase = Some(phrase);
                PermanentOperandState::Domain
            }
            PermanentOperandState::Domain => {
                cursor.enter("permanent.operand.collection.domain");
                let phrase = frame
                    .phrase
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated permanent collection"))?;
                cursor.require(
                    phrase.domain == Some(ObjectKind::Permanent),
                    "an explicitly battlefield-permanent collection, not players or cards",
                )?;
                // The operand ends transactionally: a relative clause the
                // shared query did not read is this operand's failure, not
                // the end of the operand.
                super::refuse_unread_relative_tail(cursor)?;
                PermanentOperandState::Complete
            }
            PermanentOperandState::Complete => {
                cursor.enter("permanent.operand.complete");
                if let Some(reference) = frame.reference {
                    return Ok(PermanentOperandNode::Reference(reference));
                }
                return Ok(PermanentOperandNode::Collection {
                    objects: cursor.require_value(frame.collection, "a typed permanent operand")?,
                    exclude_source: frame.exclude_source,
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "permanent_operand_tests.rs"]
mod tests;
