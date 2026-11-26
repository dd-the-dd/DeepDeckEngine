//! Lowering of the objects a definite description names
//! (`definite_object_description.rs`) into the shared object-expression
//! vocabulary.
//!
//! - an earlier action's result or the event's object: the bound object
//!   expression every reference of the program lowers to;
//! - a cost-paid object: the record its paying cost writes
//!   (`decisionResult`, `bindObjectAs`), which card composition installs on
//!   exactly one producing cost (`contextual_binding.rs`);
//! - a linked exile: the cards the paired exile of the source recorded
//!   (`cardsExiledWithSource`, `link: pair`), which card composition pairs
//!   with exactly one exiling ability of the face (`card_composition.rs`).
use super::super::*;

/// The object expression of a described object.
pub(in crate::oracle::canonical) fn lower_described_object(object: &DescribedObject) -> Value {
    match object {
        DescribedObject::Bound(reference) => bound_permanent_reference_value(*reference, &[])
            .unwrap_or_else(|| json!({ "kind": DECLARED_TARGET_ANCHOR })),
        DescribedObject::CostPaid(object) => decision_result(object.decision_id()),
        DescribedObject::LinkedExile => {
            json!({ "kind": "cardsExiledWithSource", "link": "pair" })
        }
    }
}
