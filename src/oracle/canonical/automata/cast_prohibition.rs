//! `Cast this spell only …` (CR 601.3): a casting restriction of the spell
//! itself. Static prohibitions on players casting spells are player
//! predicates of the static frame (`static_player_predicate`).
use super::super::{MachineResult, OracleTokenCursor};

/// Whether `Cast this spell only ...` starts here (CR 601.3).
pub(in crate::oracle::canonical) fn spell_cast_restriction_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ["cast", "this", "spell", "only"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// `Cast this spell only CLAUSES.`: a restriction on casting the spell
/// (CR 601.3), read by the same clause grammar as activation restrictions
/// with the clauses meaningful for a cast (turn windows and conditions).
pub(in crate::oracle::canonical) fn recognize_spell_cast_restriction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::ActivationRestrictionsNode> {
    cursor.enter("cast-restriction.head");
    cursor.consume_phrase(&["cast", "this", "spell", "only"])?;
    let mut node = super::ActivationRestrictionsNode::default();
    super::recognize_restriction_clauses(cursor, &mut node, super::RestrictedAction::Cast)?;
    cursor.enter("cast-restriction.terminator");
    cursor.consume(".")?;
    cursor.enter("cast-restriction.complete");
    cursor.finish()?;
    Ok(node)
}
