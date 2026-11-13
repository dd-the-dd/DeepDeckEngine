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
