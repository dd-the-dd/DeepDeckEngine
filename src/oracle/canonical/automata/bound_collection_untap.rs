use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type};

use super::super::{MachineResult, OracleTokenCursor};
use super::{BoundCollectionUntapEffectNode, CollectionBinding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Action,
    Determiner,
    ObjectType,
    Complete,
}

/// Recognizes `untap that <permanent type>` while retaining the collection
/// binding supplied by the parent sequence.
pub(in crate::oracle::canonical) fn recognize_untap_bound_collection(
    cursor: &mut OracleTokenCursor<'_>,
    binding: CollectionBinding,
) -> MachineResult<BoundCollectionUntapEffectNode> {
    let mut state = State::Action;
    let mut object_type = None;
    loop {
        state = match state {
            State::Action => {
                cursor.enter("effect.untap-bound-collection.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Untap,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                State::Determiner
            }
            State::Determiner => {
                cursor.enter("effect.untap-bound-collection.determiner");
                cursor.consume("that")?;
                State::ObjectType
            }
            State::ObjectType => {
                cursor.enter("effect.untap-bound-collection.object-type");
                object_type = Some(
                    cursor
                        .peek()
                        .and_then(|raw| {
                            lookup_permanent_card_type(raw, AcceptedForms::SingularOnly)
                        })
                        .ok_or_else(|| cursor.fail("a singular permanent type"))?,
                );
                cursor.advance_to(cursor.position() + 1)?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("effect.untap-bound-collection.complete");
                return Ok(BoundCollectionUntapEffectNode {
                    binding,
                    object_type: cursor
                        .require_value(object_type, "the referenced permanent type")?,
                });
            }
        };
    }
}
