use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{NumericExpression, recognize_numeric_expression};
use super::counted_card::recognize_card_count_noun;
use super::player_subject::{PlayerBinding, PlayerEffectSubject, recognize_player_effect_subject};
use super::selection_method::{CollectionSelectionMethod, recognize_postfix_selection_method};
use super::{ActingSubject, recognize_discard_action};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerDiscardEffectNode {
    pub player: PlayerEffectSubject,
    pub count: NumericExpression,
    pub selection: CollectionSelectionMethod,
}

/// Resolve a singular subject, then delegate the same action tail that
/// quantified player parents use. Neither reader consumes sequence connectors.
pub(in crate::oracle::canonical::automata) fn recognize_player_discard_effect(
    cursor: &mut OracleTokenCursor<'_>,
    declared_target_binding: PlayerBinding,
    antecedent: Option<PlayerBinding>,
) -> MachineResult<PlayerDiscardEffectNode> {
    cursor.enter("effect.player-discard.subject");
    let player =
        recognize_player_effect_subject(cursor, declared_target_binding, antecedent, false)?;
    let form = match player {
        PlayerEffectSubject::Controller => ActingSubject::CONTROLLER,
        _ => ActingSubject::THIRD_SINGULAR,
    };
    recognize_player_discard_action_fragment(cursor, player, form)
}

pub(in crate::oracle::canonical::automata) fn recognize_player_discard_action_fragment(
    cursor: &mut OracleTokenCursor<'_>,
    player: PlayerEffectSubject,
    form: ActingSubject,
) -> MachineResult<PlayerDiscardEffectNode> {
    cursor.enter("effect.player-discard.action");
    let action = recognize_discard_action(cursor, form, |cursor| {
        cursor.enter("effect.player-discard.count");
        let count = recognize_numeric_expression(cursor, None)?;
        recognize_card_count_noun(cursor, &count)?;
        Ok(count)
    })?;
    cursor.enter("effect.player-discard.selection-method");
    let selection = recognize_postfix_selection_method(cursor)?;
    cursor.enter("effect.player-discard.complete");
    Ok(PlayerDiscardEffectNode {
        player,
        count: action.operand,
        selection,
    })
}

#[cfg(test)]
#[path = "player_discard_tests.rs"]
mod tests;
