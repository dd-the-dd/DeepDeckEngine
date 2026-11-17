use super::super::{MachineFailure, OracleTokenCursor};
use super::RecognizedEffectFragment;

/// Unbound grammatical roles. The parent resolves lexical scope and set subtraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardCollectionReferenceNode {
    Antecedent,
    Remainder,
    Other,
    Selected,
}

pub(in crate::oracle::canonical) enum CollectionOperandAttempt<T> {
    NotClaimed,
    Recognized(RecognizedEffectFragment<T>),
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
}

pub(in crate::oracle::canonical) type CardCollectionReferenceAttempt =
    CollectionOperandAttempt<CardCollectionReferenceNode>;

/// A singular complement is a role, not a runtime demand to find one card.
pub(in crate::oracle::canonical) fn try_recognize_card_collection_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> CardCollectionReferenceAttempt {
    let mut trial = cursor.clone();
    trial.enter("collection.reference.head");
    let node = match trial.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("them") => {
            if let Err(f) = trial.consume("them") {
                return CollectionOperandAttempt::FailedCommitted(f);
            }
            CardCollectionReferenceNode::Antecedent
        }
        Some("it") => {
            if let Err(f) = trial.consume("it") {
                return CollectionOperandAttempt::FailedCommitted(f);
            }
            CardCollectionReferenceNode::Selected
        }
        Some("those") => {
            if let Err(f) = trial.consume_phrase(&["those", "cards"]) {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Antecedent
        }
        Some("that") => {
            if let Err(f) = trial.consume_phrase(&["that", "card"]) {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            CardCollectionReferenceNode::Selected
        }
        Some("the") => {
            if let Err(f) = trial.consume("the") {
                return CollectionOperandAttempt::RejectedCandidate(f);
            }
            match trial.peek().map(str::to_ascii_lowercase).as_deref() {
                Some("rest") => {
                    if let Err(f) = trial.consume("rest") {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    CardCollectionReferenceNode::Remainder
                }
                Some("other") => {
                    if let Err(f) = trial.consume("other") {
                        return CollectionOperandAttempt::FailedCommitted(f);
                    }
                    CardCollectionReferenceNode::Other
                }
                Some("cards") => {
                    if let Err(f) = trial.consume("cards") {
                        return CollectionOperandAttempt::RejectedCandidate(f);
                    }
                    CardCollectionReferenceNode::Antecedent
                }
                _ => {
                    return CollectionOperandAttempt::RejectedCandidate(
                        trial.fail("rest, other or cards after the collection article"),
                    );
                }
            }
        }
        _ => return CollectionOperandAttempt::NotClaimed,
    };
    trial.enter("collection.reference.complete");
    let end_position = trial.position();
    *cursor = trial;
    CollectionOperandAttempt::Recognized(RecognizedEffectFragment { node, end_position })
}

#[cfg(test)]
#[path = "card_collection_operand_tests.rs"]
mod tests;
