use super::super::super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CollectionSelectionMethod {
    #[default]
    Choice,
    Random,
}

/// Recognizes the postfix Oracle modifier `at random`. Its absence preserves
/// ordinary player choice. Candidate collection, cardinality, and destination
/// remain independent responsibilities of the parent effect.
pub(in crate::oracle::canonical::automata) fn recognize_postfix_selection_method(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CollectionSelectionMethod> {
    cursor.enter("selection-method.start");
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("at"))
    {
        cursor.enter("selection-method.choice");
        return Ok(CollectionSelectionMethod::Choice);
    }

    cursor.enter("selection-method.random.relation");
    cursor.consume("at")?;
    cursor.enter("selection-method.random.mode");
    cursor.consume("random")?;
    cursor.enter("selection-method.complete");
    Ok(CollectionSelectionMethod::Random)
}

/// Optional explicit chooser marker `of their choice`: the player performing
/// the action makes the selection (CR 101.4 order is owned by the parent's
/// iteration). Absence has the same meaning for actions whose actor always
/// chooses, so the marker contributes no semantics beyond being consumed as one
/// unit; returns whether it was present.
pub(in crate::oracle::canonical::automata) fn recognize_acting_player_choice_marker(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    cursor.enter("selection-method.chooser.start");
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
    {
        return Ok(false);
    }
    cursor.enter("selection-method.chooser.marker");
    cursor.consume_phrase(&["of", "their", "choice"])?;
    cursor.enter("selection-method.chooser.complete");
    Ok(true)
}
