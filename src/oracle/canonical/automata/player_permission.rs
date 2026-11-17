//! Player allowances share their actor/count header across lifetime parents.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TemporaryPlayerPermissionNode {
    pub modifier: PlayerModifierNode,
    pub duration: EffectDurationNode,
}

pub(in crate::oracle::canonical) enum TemporaryPlayerPermissionParseAttempt {
    NotClaimed,
    RejectedCandidate(MachineFailure),
    FailedCommitted(MachineFailure),
    Parsed(TemporaryPlayerPermissionNode),
}

/// The lifetime and sentence boundary belong to the caller, not this header.
pub(in crate::oracle::canonical) fn recognize_additional_land_allowance_header(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerModifierNode> {
    cursor.enter("modifier.additional-land-play.player");
    let player = recognize_player_reference(cursor, None)?;
    cursor.enter("modifier.additional-land-play.permission");
    cursor.consume("may")?;
    cursor.enter("modifier.additional-land-play.action");
    cursor.consume("play")?;
    cursor.enter("modifier.additional-land-play.quantity");
    let count = recognize_numeric_expression(cursor, None)?;
    cursor.enter("modifier.additional-land-play.additional");
    cursor.consume("additional")?;
    cursor.enter("modifier.additional-land-play.land");
    let noun = if count == NumericExpression::Atom(NumericAtom::Integer(1)) {
        "land"
    } else {
        "lands"
    };
    cursor.consume(noun)?;
    Ok(PlayerModifierNode::AdditionalLandPlay { player, count })
}

/// A typed allowance plus a temporary-duration head belongs to the effect
/// parent. This bounded probe prevents the persistent static family stealing
/// that child. It reads shared tokens, restores the checkpoint, and never
/// guesses ownership from how far a failed child advanced.
pub(in crate::oracle::canonical) fn temporary_land_permission_owns_head(
    cursor: &mut OracleTokenCursor<'_>,
) -> bool {
    let checkpoint = cursor.checkpoint();
    let owns = recognize_additional_land_allowance_header(cursor).is_ok()
        && cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("this") || raw.eq_ignore_ascii_case("until")
        });
    cursor.rollback(checkpoint);
    owns
}

pub(in crate::oracle::canonical) fn try_recognize_temporary_player_permission(
    cursor: &mut OracleTokenCursor<'_>,
) -> TemporaryPlayerPermissionParseAttempt {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        || !cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"))
        || !cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("play"))
    {
        return TemporaryPlayerPermissionParseAttempt::NotClaimed;
    }
    let checkpoint = cursor.checkpoint();
    let modifier = match cursor.attempt(recognize_additional_land_allowance_header) {
        Ok(modifier) => modifier,
        Err(failure) => return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure),
    };
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("on"))
    {
        let failure =
            cursor.fail("a temporary effect duration rather than a persistent per-turn allowance");
        cursor.rollback(checkpoint);
        return TemporaryPlayerPermissionParseAttempt::RejectedCandidate(failure);
    }
    // A complete typed header commits its explicit temporary lifetime.
    cursor.enter("permission.temporary.duration");
    match recognize_effect_duration(cursor) {
        Ok(duration) => {
            TemporaryPlayerPermissionParseAttempt::Parsed(TemporaryPlayerPermissionNode {
                modifier,
                duration,
            })
        }
        Err(failure) => TemporaryPlayerPermissionParseAttempt::FailedCommitted(failure),
    }
}

#[cfg(test)]
#[path = "player_permission_tests.rs"]
mod tests;
