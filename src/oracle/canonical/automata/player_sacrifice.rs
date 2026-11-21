//! `sacrifices COUNT QUERY [of their choice]` performed by one acting player on
//! its own permanents. Singular and quantified subjects share this action tail;
//! the parent owns the subject, its population and every connector.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    DiscardVerbForm, recognize_sacrifice_action_with_form,
    selection_method::recognize_acting_player_choice_marker,
};
use super::{
    NumericExpression, ObjectQueryNode, SacrificeEffectNode, SacrificeOperandOwner,
    recognize_selected_sacrifice_operand,
};

/// The shared quantity and query own the operand; the chooser marker is only
/// recorded, because the acting player always chooses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerSacrificeActionNode {
    pub count: NumericExpression,
    pub object: ObjectQueryNode,
    pub explicit_choice: bool,
}

pub(in crate::oracle::canonical::automata) fn recognize_player_sacrifice_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSacrificeActionNode> {
    recognize_player_sacrifice_action_with_form(cursor, DiscardVerbForm::ThirdPersonSingular)
}

/// The same action tail with the verb form chosen by the subject's frame:
/// finite `sacrifices`, or the bare `sacrifice` after a causative `have`.
pub(in crate::oracle::canonical::automata) fn recognize_player_sacrifice_action_with_form(
    cursor: &mut OracleTokenCursor<'_>,
    verb_form: DiscardVerbForm,
) -> MachineResult<PlayerSacrificeActionNode> {
    let action = recognize_sacrifice_action_with_form(cursor, verb_form, |cursor| {
        recognize_selected_sacrifice_operand(cursor, SacrificeOperandOwner::ActingPlayer)
    })?;
    cursor.enter("player-sacrifice.chooser");
    let explicit_choice = recognize_acting_player_choice_marker(cursor)?;
    let SacrificeEffectNode::Selected { object, count } = action.operand else {
        return Err(cursor.fail("a selected sacrifice operand"));
    };
    cursor.enter("player-sacrifice.complete");
    Ok(PlayerSacrificeActionNode {
        count,
        object,
        explicit_choice,
    })
}
