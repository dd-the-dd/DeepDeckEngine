use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LibraryPosition, LocalPlayerReference, OptionalShuffleCollectionNode,
    ReorderBoundCollectionNode, TopCardsEffectNode, TopCardsReorderSequenceNode,
    recognize_collection_order, recognize_top_cards_effect,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReorderState {
    Connector,
    Action,
    Reference,
    Return,
    Order,
    Terminator,
    Complete,
}

fn recognize_reorder_bound_collection(
    cursor: &mut OracleTokenCursor<'_>,
    top: &TopCardsEffectNode,
) -> MachineResult<ReorderBoundCollectionNode> {
    let mut state = ReorderState::Connector;
    let mut order = None;
    loop {
        state = match state {
            ReorderState::Connector => {
                cursor.enter("sequence.top-cards-reorder.connector");
                cursor.consume_phrase(&[",", "then"])?;
                ReorderState::Action
            }
            ReorderState::Action => {
                cursor.enter("effect.reorder-collection.action");
                cursor.consume("put")?;
                ReorderState::Reference
            }
            ReorderState::Reference => {
                cursor.enter("reference.collection.bound");
                if top.count == 1 {
                    cursor.consume("it")?;
                } else {
                    cursor.consume("them")?;
                }
                ReorderState::Return
            }
            ReorderState::Return => {
                cursor.enter("effect.reorder-collection.return");
                cursor.consume("back")?;
                ReorderState::Order
            }
            ReorderState::Order => {
                cursor.enter("effect.reorder-collection.order");
                order = Some(recognize_collection_order(cursor)?);
                ReorderState::Terminator
            }
            ReorderState::Terminator => {
                cursor.enter("effect.reorder-collection.terminator");
                cursor.consume(".")?;
                ReorderState::Complete
            }
            ReorderState::Complete => {
                cursor.enter("effect.reorder-collection.complete");
                return Ok(ReorderBoundCollectionNode {
                    binding: top.binding,
                    destination: top.zone,
                    position: LibraryPosition::Top,
                    order: cursor.require_value(order, "a collection order")?,
                });
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionalShuffleState {
    Player,
    Modality,
    Action,
    Terminator,
    Complete,
}

fn recognize_optional_shuffle_collection(
    cursor: &mut OracleTokenCursor<'_>,
    top: &TopCardsEffectNode,
) -> MachineResult<OptionalShuffleCollectionNode> {
    let mut state = OptionalShuffleState::Player;
    loop {
        state = match state {
            OptionalShuffleState::Player => {
                cursor.enter("effect.optional-shuffle.player");
                cursor.consume("you")?;
                OptionalShuffleState::Modality
            }
            OptionalShuffleState::Modality => {
                cursor.enter("effect.optional-shuffle.modality");
                cursor.consume("may")?;
                OptionalShuffleState::Action
            }
            OptionalShuffleState::Action => {
                cursor.enter("effect.optional-shuffle.action");
                cursor.consume("shuffle")?;
                OptionalShuffleState::Terminator
            }
            OptionalShuffleState::Terminator => {
                cursor.enter("effect.optional-shuffle.terminator");
                cursor.consume(".")?;
                OptionalShuffleState::Complete
            }
            OptionalShuffleState::Complete => {
                cursor.enter("effect.optional-shuffle.complete");
                return Ok(OptionalShuffleCollectionNode {
                    player: LocalPlayerReference::Controller,
                    zone: top.zone,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_top_cards_reorder_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TopCardsReorderSequenceNode> {
    cursor.enter("sequence.top-cards-reorder.look");
    let top = recognize_top_cards_effect(cursor)?;
    cursor.enter("sequence.top-cards-reorder.reorder");
    let reorder = recognize_reorder_bound_collection(cursor, &top)?;
    cursor.enter("sequence.top-cards-reorder.optional-shuffle");
    let shuffle = recognize_optional_shuffle_collection(cursor, &top)?;
    cursor.enter("sequence.top-cards-reorder.complete");
    cursor.finish()?;
    Ok(TopCardsReorderSequenceNode {
        top,
        reorder,
        shuffle,
    })
}
