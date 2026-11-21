//! Causative `have SUBJECT VERB`: the controller has another object or player
//! perform an ordinary action (`you may have target opponent sacrifice a
//! creature`, `you may have it deal 4 damage to any target`). The causative
//! names no action of its own. The subject-verb readers keep their typed
//! subjects and arguments; only the verb agreement becomes the bare infinitive
//! (`SubjectVerbAgreement::BareInfinitive`). A subject or verb that no reader
//! owns fails at that reader's state, never as an approximated action.
use super::super::OracleTokenCursor;

/// True when the action position opens with the causative `have` followed by
/// a subject. In an effect program `have` has no possessive reading: static
/// possession (`creatures you control have flying`) follows its subject.
pub(in crate::oracle::canonical) fn causative_action_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("have"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.chars().next().is_some_and(char::is_alphanumeric))
}

#[cfg(test)]
#[path = "causative_tests.rs"]
mod tests;
