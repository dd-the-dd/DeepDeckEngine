use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::LocalPlayerReference;
use super::PlayerModifierNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoMaximumHandSizeState {
    Player,
    Predicate,
    Negation,
    Maximum,
    Hand,
    Size,
    Terminator,
    Complete,
}

/// Recognizes the player modifier independently of the static-ability or
/// quoted-emblem parent that contains it.
pub(super) fn recognize_no_maximum_hand_size_modifier(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerModifierNode> {
    let mut state = NoMaximumHandSizeState::Player;
    loop {
        state = match state {
            NoMaximumHandSizeState::Player => {
                cursor.enter("modifier.no-maximum-hand-size.player");
                cursor.consume("you")?;
                NoMaximumHandSizeState::Predicate
            }
            NoMaximumHandSizeState::Predicate => {
                cursor.enter("modifier.no-maximum-hand-size.predicate");
                cursor.consume("have")?;
                NoMaximumHandSizeState::Negation
            }
            NoMaximumHandSizeState::Negation => {
                cursor.enter("modifier.no-maximum-hand-size.negation");
                cursor.consume("no")?;
                NoMaximumHandSizeState::Maximum
            }
            NoMaximumHandSizeState::Maximum => {
                cursor.enter("modifier.no-maximum-hand-size.maximum");
                cursor.consume("maximum")?;
                NoMaximumHandSizeState::Hand
            }
            NoMaximumHandSizeState::Hand => {
                cursor.enter("modifier.no-maximum-hand-size.hand");
                cursor.consume("hand")?;
                NoMaximumHandSizeState::Size
            }
            NoMaximumHandSizeState::Size => {
                cursor.enter("modifier.no-maximum-hand-size.size");
                cursor.consume("size")?;
                NoMaximumHandSizeState::Terminator
            }
            NoMaximumHandSizeState::Terminator => {
                cursor.enter("modifier.no-maximum-hand-size.terminator");
                cursor.consume(".")?;
                NoMaximumHandSizeState::Complete
            }
            NoMaximumHandSizeState::Complete => {
                cursor.enter("modifier.no-maximum-hand-size.complete");
                return Ok(PlayerModifierNode::NoMaximumHandSize {
                    player: LocalPlayerReference::Controller,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn recognize_static_no_maximum_hand_size(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerModifierNode> {
    let modifier = recognize_no_maximum_hand_size_modifier(cursor)?;
    cursor.enter("static.no-maximum-hand-size.complete");
    cursor.finish()?;
    Ok(modifier)
}

#[cfg(test)]
#[path = "no_maximum_hand_size_tests.rs"]
mod tests;
