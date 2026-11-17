//! Shared player populations independent of the action applied to them.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerSetReferenceNode {
    EachPlayer,
    OpponentsOfAbilityController,
}

pub(in crate::oracle::canonical) fn recognize_player_set_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSetReferenceNode> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
    {
        cursor.enter("player-set.quantifier");
        cursor.consume("each")?;
        cursor.enter("player-set.role");
        let role = if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("opponent"))
        {
            cursor.consume("opponent")?;
            PlayerSetReferenceNode::OpponentsOfAbilityController
        } else {
            cursor.consume("player")?;
            PlayerSetReferenceNode::EachPlayer
        };
        cursor.enter("player-set.complete");
        return Ok(role);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("players"))
    {
        cursor.enter("player-set.role");
        cursor.consume("players")?;
        cursor.enter("player-set.complete");
        return Ok(PlayerSetReferenceNode::EachPlayer);
    }
    // Preserve the historical opponent reader's states and failure boundary.
    cursor.enter("player-set.possessor");
    cursor.consume("your")?;
    cursor.enter("player-set.role");
    cursor.consume("opponents")?;
    cursor.enter("player-set.complete");
    Ok(PlayerSetReferenceNode::OpponentsOfAbilityController)
}

#[cfg(test)]
#[path = "player_set_tests.rs"]
mod tests;

/// A zone holder is access scope, never an ownership predicate or target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneHolderQueryNode {
    Player(super::LocalPlayerReference),
    Players(PlayerSetReferenceNode),
}

pub(in crate::oracle::canonical) fn recognize_zone_holder_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ZoneHolderQueryNode> {
    cursor.enter("query.zone.holder");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        cursor.consume("your")?;
        Ok(ZoneHolderQueryNode::Player(
            super::LocalPlayerReference::Controller,
        ))
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
    {
        cursor.consume("all")?;
        Ok(ZoneHolderQueryNode::Players(
            PlayerSetReferenceNode::EachPlayer,
        ))
    } else if {
        let mut lexical = cursor.clone();
        super::recognize_card_zone_atom(&mut lexical)
            .ok()
            .is_some_and(|zone| {
                cursor
                    .peek()
                    .is_some_and(|raw| !raw.eq_ignore_ascii_case(zone.ir_name()))
            })
    } {
        // A plural zone noun denotes the holder population. The zone child
        // owns the actual noun and proves its lexical kind.
        Ok(ZoneHolderQueryNode::Players(
            PlayerSetReferenceNode::EachPlayer,
        ))
    } else {
        Err(cursor.fail("your zone or all player zones; zone targeting/selection requires an explicit typed producer"))
    }
}
