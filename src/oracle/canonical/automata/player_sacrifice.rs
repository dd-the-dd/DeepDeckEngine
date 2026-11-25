//! `sacrifices COUNT QUERY [of their choice]` performed by one acting player on
//! its own permanents. Singular and quantified subjects share this action tail;
//! the parent owns the subject, its population and every connector.
use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    ActingSubject, recognize_sacrifice_action_with_form,
    selection_method::recognize_acting_player_choice_marker,
};
use super::{
    NumericAtom, NumericExpression, ObjectQueryContext, ObjectQueryNode, SacrificeEffectNode,
    SacrificeOperandOwner, SelectionCount, continue_unrelated_predicate,
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
    recognize_player_sacrifice_action_with_form(cursor, ActingSubject::THIRD_SINGULAR)
}

/// The same action tail with the verb form chosen by the subject's frame:
/// finite `sacrifices`, or the bare `sacrifice` after a causative `have`.
pub(in crate::oracle::canonical::automata) fn recognize_player_sacrifice_action_with_form(
    cursor: &mut OracleTokenCursor<'_>,
    verb_form: ActingSubject,
) -> MachineResult<PlayerSacrificeActionNode> {
    let action = recognize_sacrifice_action_with_form(cursor, verb_form, |cursor| {
        recognize_selected_sacrifice_operand(cursor, SacrificeOperandOwner::ActingPlayer)
    })?;
    cursor.enter("player-sacrifice.chooser");
    let explicit_choice = recognize_acting_player_choice_marker(cursor)?;
    let SacrificeEffectNode::Selected {
        mut object,
        count: SelectionCount::Exactly(count),
    } = action.operand
    else {
        return Err(cursor.fail("a selected sacrifice operand with a stated count"));
    };
    if explicit_choice {
        // `a creature of their choice with flying`: the shared postnominal
        // list of the operand continues after the chooser marker.
        cursor.enter("player-sacrifice.postnominal");
        let plural = !matches!(count, NumericExpression::Atom(NumericAtom::Integer(1)));
        object.predicate = continue_unrelated_predicate(
            cursor,
            ObjectQueryContext::ORDINARY,
            object.predicate,
            plural,
        )?;
    }
    cursor.enter("player-sacrifice.complete");
    Ok(PlayerSacrificeActionNode {
        count,
        object,
        explicit_choice,
    })
}
