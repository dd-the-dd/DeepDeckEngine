use super::super::{MachineResult, OracleTokenCursor};

/// The excluded object is the bound source, not a negated card predicate or
/// every object with the source's name. Parents retain its semantic binding.
pub(in crate::oracle::canonical) fn recognize_optional_source_exclusion(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("other"))
    {
        cursor.consume("other")?;
        Ok(true)
    } else {
        Ok(false)
    }
}
