use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalPlayerReference, NumericExpression};

mod additional_land_play;
mod no_maximum_hand_size;

pub(in crate::oracle::canonical) use additional_land_play::recognize_static_additional_land_play;
use no_maximum_hand_size::recognize_no_maximum_hand_size_modifier;
pub(in crate::oracle::canonical) use no_maximum_hand_size::recognize_static_no_maximum_hand_size;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerModifierNode {
    AdditionalLandPlay {
        player: LocalPlayerReference,
        count: NumericExpression,
    },
    NoMaximumHandSize {
        player: LocalPlayerReference,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CreateEmblemEffectNode {
    pub player: LocalPlayerReference,
    pub modifiers: Vec<PlayerModifierNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmblemState {
    Player,
    Action,
    Article,
    Object,
    Relation,
    QuoteOpen,
    Modifier,
    QuoteClose,
    Complete,
}

/// Recognizes emblem creation separately from the player modifier quoted on
/// the emblem. More modifier variants can be added without changing the
/// emblem envelope.
pub(in crate::oracle::canonical) fn recognize_create_emblem_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CreateEmblemEffectNode> {
    let mut state = EmblemState::Player;
    loop {
        state = match state {
            EmblemState::Player => {
                cursor.enter("effect.create-emblem.player");
                cursor.consume("you")?;
                EmblemState::Action
            }
            EmblemState::Action => {
                cursor.enter("effect.create-emblem.action");
                cursor.consume("get")?;
                EmblemState::Article
            }
            EmblemState::Article => {
                cursor.enter("effect.create-emblem.article");
                cursor.consume("an")?;
                EmblemState::Object
            }
            EmblemState::Object => {
                cursor.enter("effect.create-emblem.object");
                cursor.consume("emblem")?;
                EmblemState::Relation
            }
            EmblemState::Relation => {
                cursor.enter("effect.create-emblem.relation");
                cursor.consume("with")?;
                EmblemState::QuoteOpen
            }
            EmblemState::QuoteOpen => {
                cursor.enter("effect.create-emblem.quote-open");
                cursor.consume("\"")?;
                EmblemState::Modifier
            }
            EmblemState::Modifier => {
                recognize_no_maximum_hand_size_modifier(cursor)?;
                EmblemState::QuoteClose
            }
            EmblemState::QuoteClose => {
                cursor.enter("effect.create-emblem.quote-close");
                cursor.consume("\"")?;
                EmblemState::Complete
            }
            EmblemState::Complete => {
                cursor.enter("effect.create-emblem.complete");
                return Ok(CreateEmblemEffectNode {
                    player: LocalPlayerReference::Controller,
                    modifiers: vec![PlayerModifierNode::NoMaximumHandSize {
                        player: LocalPlayerReference::Controller,
                    }],
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "emblem_tests.rs"]
mod tests;
