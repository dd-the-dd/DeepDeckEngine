use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerSetReferenceNode {
    OpponentsOfAbilityController,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TurnContextNode {
    AbilityControllerTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastProhibitionNode {
    pub players: PlayerSetReferenceNode,
    pub turn: TurnContextNode,
}

/// Recognizes a plural player set without assigning any prohibited action to
/// it. Other permission grammars can reuse this relation independently.
pub(in crate::oracle::canonical) fn recognize_player_set_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSetReferenceNode> {
    cursor.enter("player-set.possessor");
    cursor.consume("your")?;
    cursor.enter("player-set.role");
    cursor.consume("opponents")?;
    cursor.enter("player-set.complete");
    Ok(PlayerSetReferenceNode::OpponentsOfAbilityController)
}

/// Recognizes whose turn supplies a static timing context. The subject is
/// typed rather than retained as source text.
pub(in crate::oracle::canonical) fn recognize_turn_context(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TurnContextNode> {
    cursor.enter("turn-context.possessor");
    cursor.consume("your")?;
    cursor.enter("turn-context.noun");
    cursor.consume("turn")?;
    cursor.enter("turn-context.complete");
    Ok(TurnContextNode::AbilityControllerTurn)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CastProhibitionState {
    Players,
    Prohibition,
    Action,
    Object,
    TimingRelation,
    Turn,
    Terminator,
    Complete,
}

#[derive(Default)]
struct CastProhibitionFrame {
    players: Option<PlayerSetReferenceNode>,
    turn: Option<TurnContextNode>,
}

/// Recognizes `PLAYER_SET can't/cannot cast spells during TURN_CONTEXT` as a
/// static permission modifier. Player and turn references are delegated to
/// independent children and committed only after complete consumption.
pub(in crate::oracle::canonical) fn recognize_cast_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CastProhibitionNode> {
    let mut state = CastProhibitionState::Players;
    let mut frame = CastProhibitionFrame::default();
    loop {
        state = match state {
            CastProhibitionState::Players => {
                cursor.enter("cast-prohibition.players");
                frame.players = Some(recognize_player_set_reference(cursor)?);
                CastProhibitionState::Prohibition
            }
            CastProhibitionState::Prohibition => {
                cursor.enter("cast-prohibition.prohibition");
                let raw = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("cannot")
                    })
                    .ok_or_else(|| cursor.fail("`can't` or `cannot`"))?
                    .to_string();
                cursor.consume(&raw)?;
                CastProhibitionState::Action
            }
            CastProhibitionState::Action => {
                cursor.enter("cast-prohibition.action");
                cursor.consume("cast")?;
                CastProhibitionState::Object
            }
            CastProhibitionState::Object => {
                cursor.enter("cast-prohibition.object");
                cursor.consume("spells")?;
                CastProhibitionState::TimingRelation
            }
            CastProhibitionState::TimingRelation => {
                cursor.enter("cast-prohibition.timing-relation");
                cursor.consume("during")?;
                CastProhibitionState::Turn
            }
            CastProhibitionState::Turn => {
                cursor.enter("cast-prohibition.turn");
                frame.turn = Some(recognize_turn_context(cursor)?);
                CastProhibitionState::Terminator
            }
            CastProhibitionState::Terminator => {
                cursor.enter("cast-prohibition.terminator");
                cursor.consume(".")?;
                CastProhibitionState::Complete
            }
            CastProhibitionState::Complete => {
                cursor.enter("cast-prohibition.complete");
                cursor.finish()?;
                return Ok(CastProhibitionNode {
                    players: cursor.require_value(frame.players, "a prohibited player set")?,
                    turn: cursor.require_value(frame.turn, "a prohibition turn context")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "cast_prohibition_tests.rs"]
mod tests;
