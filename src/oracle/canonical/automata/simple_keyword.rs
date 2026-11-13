use super::super::{MachineResult, OracleTokenCursor};
use super::{KeywordListNode, recognize_standalone_keyword_list};

/// Recognizes a complete standalone keyword ability by delegating every atom
/// and list connector to the shared keyword-list automaton.
pub(in crate::oracle::canonical) fn recognize_standalone_keyword_ability(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<KeywordListNode> {
    cursor.enter("keyword.standalone.list");
    let list = recognize_standalone_keyword_list(cursor)?;
    cursor.enter("keyword.standalone.complete");
    cursor.finish()?;
    Ok(list)
}

#[cfg(test)]
#[path = "simple_keyword_tests.rs"]
mod tests;
