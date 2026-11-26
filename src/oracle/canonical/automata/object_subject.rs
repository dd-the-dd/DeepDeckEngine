//! Subject-cell shapes of object-subject effect clauses that are neither a
//! reference nor a declaration:
//!
//! ```text
//! StatusCollection := ("enchanted" | "equipped") PLURAL_NOUN QUERY_TAIL
//! ```
//!
//! `enchanted creature` / `equipped creature` is the reference to the one
//! permanent the source is attached to (CR 301.5a, 303.4a). With a plural
//! noun the same adjective is the attachment status of every member of an
//! ordinary collection (`Equipped creatures you control gain indestructible
//! until end of turn`, CR 301.5, 303.4): the shared object query reads it as
//! its status predicate, with its own relations, never as a reference.
use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_card_type};

use super::super::{MachineResult, OracleTokenCursor};
use super::{PermanentOperandNode, recognize_object_collection};

/// A plural object noun at `offset`: a card type written in its plural form,
/// or `permanents`.
fn plural_object_noun_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        raw.eq_ignore_ascii_case("permanents")
            || (lookup_card_type(raw, AcceptedForms::SingularOrPlural).is_some()
                && lookup_card_type(raw, AcceptedForms::SingularOnly).is_none())
    })
}

/// `equipped creatures`, `enchanted permanents`: an attachment adjective
/// before a plural noun starts a status collection.
pub(in crate::oracle::canonical) fn attached_status_collection_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("enchanted") || raw.eq_ignore_ascii_case("equipped")
    }) && plural_object_noun_at(cursor, 1)
}

/// Reads the status collection as the ordinary battlefield collection.
pub(in crate::oracle::canonical) fn recognize_attached_status_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PermanentOperandNode> {
    cursor.enter("permanent.subject.attachment-status-collection");
    let objects = recognize_object_collection(cursor)?;
    cursor.require(
        objects.denotes_battlefield_permanents(),
        "a battlefield permanent collection",
    )?;
    let exclude_source = objects.exclude_source;
    Ok(PermanentOperandNode::Collection {
        objects,
        exclude_source,
    })
}

#[cfg(test)]
#[path = "object_subject_tests.rs"]
mod tests;
