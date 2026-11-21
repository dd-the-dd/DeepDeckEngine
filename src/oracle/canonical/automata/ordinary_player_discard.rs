//! Ordinary controller discard is one composable effect, independent of costs.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    ActingSubject,
    player_discard::{recognize_player_discard_action_fragment, recognize_player_discard_effect},
};
use super::{
    NumericAtom, NumericExpression, PlayerBinding, PlayerDiscardEffectNode, PlayerEffectSubject,
};

/// The existing action child owns verb/count/card agreement and selection.
/// This parent binds only the controller; enclosing programs own connectors,
/// declarations and full consumption. It does not create a target binding.
pub(in crate::oracle::canonical) fn recognize_ordinary_player_discard(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerDiscardEffectNode> {
    let node = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
    {
        recognize_player_discard_effect(cursor, PlayerBinding(0), None)?
    } else {
        recognize_player_discard_action_fragment(
            cursor,
            PlayerEffectSubject::Controller,
            ActingSubject::CONTROLLER,
        )?
    };
    cursor.enter("effect.ordinary-discard.actor");
    cursor.require(
        node.player == PlayerEffectSubject::Controller,
        "the ability controller as the ordinary discard actor",
    )?;
    cursor.enter("effect.ordinary-discard.count");
    cursor.require(
        !matches!(&node.count, NumericExpression::Atom(NumericAtom::Integer(n)) if *n < 0),
        "a nonnegative discard count",
    )?;
    cursor.enter("effect.ordinary-discard.complete");
    Ok(node)
}

#[cfg(test)]
#[path = "ordinary_player_discard_tests.rs"]
mod tests;
