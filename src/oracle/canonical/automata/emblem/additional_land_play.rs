use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::{
    LocalPlayerReference, NumericAtom, NumericExpression, recognize_numeric_expression,
    recognize_player_reference,
};
use super::PlayerModifierNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdditionalLandPlayState {
    Player,
    Permission,
    Action,
    Quantity,
    Additional,
    Land,
    PeriodOn,
    PeriodEach,
    PeriodOf,
    PeriodPlayer,
    PeriodTurns,
    Terminator,
    Complete,
}

fn recognize_quantity(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumericExpression> {
    cursor.enter("modifier.additional-land-play.quantity");
    if cursor
        .peek()
        .is_some_and(|token| token.eq_ignore_ascii_case("an"))
    {
        cursor.consume("an")?;
        return Ok(NumericExpression::Atom(NumericAtom::Integer(1)));
    }
    recognize_numeric_expression(cursor, None)
}

fn expected_land_noun(count: &NumericExpression) -> &'static str {
    if matches!(count, NumericExpression::Atom(NumericAtom::Integer(1))) {
        "land"
    } else {
        "lands"
    }
}

/// Recognizes a persistent player modifier of the form
/// `PLAYER may play QUANTITY additional land(s) on each of PLAYER's turns`.
/// The static parent currently accepts only the ability controller, but the
/// player and numeric children remain typed so other parents can reuse them.
pub(in crate::oracle::canonical) fn recognize_static_additional_land_play(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerModifierNode> {
    let mut state = AdditionalLandPlayState::Player;
    let mut player = None;
    let mut count = None;
    loop {
        state = match state {
            AdditionalLandPlayState::Player => {
                cursor.enter("modifier.additional-land-play.player");
                player = Some(recognize_player_reference(cursor, None)?);
                AdditionalLandPlayState::Permission
            }
            AdditionalLandPlayState::Permission => {
                cursor.enter("modifier.additional-land-play.permission");
                cursor.consume("may")?;
                AdditionalLandPlayState::Action
            }
            AdditionalLandPlayState::Action => {
                cursor.enter("modifier.additional-land-play.action");
                cursor.consume("play")?;
                AdditionalLandPlayState::Quantity
            }
            AdditionalLandPlayState::Quantity => {
                count = Some(recognize_quantity(cursor)?);
                AdditionalLandPlayState::Additional
            }
            AdditionalLandPlayState::Additional => {
                cursor.enter("modifier.additional-land-play.additional");
                cursor.consume("additional")?;
                AdditionalLandPlayState::Land
            }
            AdditionalLandPlayState::Land => {
                cursor.enter("modifier.additional-land-play.land");
                let count = cursor.require_value(count.as_ref(), "an additional-land count")?;
                cursor.consume(expected_land_noun(count))?;
                AdditionalLandPlayState::PeriodOn
            }
            AdditionalLandPlayState::PeriodOn => {
                cursor.enter("modifier.additional-land-play.period.on");
                cursor.consume("on")?;
                AdditionalLandPlayState::PeriodEach
            }
            AdditionalLandPlayState::PeriodEach => {
                cursor.enter("modifier.additional-land-play.period.each");
                cursor.consume("each")?;
                AdditionalLandPlayState::PeriodOf
            }
            AdditionalLandPlayState::PeriodOf => {
                cursor.enter("modifier.additional-land-play.period.of");
                cursor.consume("of")?;
                AdditionalLandPlayState::PeriodPlayer
            }
            AdditionalLandPlayState::PeriodPlayer => {
                cursor.enter("modifier.additional-land-play.period.player");
                cursor.require(
                    player == Some(LocalPlayerReference::Controller),
                    "the ability controller as the additional-land player",
                )?;
                cursor.consume("your")?;
                AdditionalLandPlayState::PeriodTurns
            }
            AdditionalLandPlayState::PeriodTurns => {
                cursor.enter("modifier.additional-land-play.period.turns");
                let period = cursor.consume_type("repetitionUnitWord")?;
                cursor.require(
                    period.raw.eq_ignore_ascii_case("turns"),
                    "the plural `turns` period",
                )?;
                AdditionalLandPlayState::Terminator
            }
            AdditionalLandPlayState::Terminator => {
                cursor.enter("modifier.additional-land-play.terminator");
                cursor.consume(".")?;
                AdditionalLandPlayState::Complete
            }
            AdditionalLandPlayState::Complete => {
                cursor.enter("modifier.additional-land-play.complete");
                cursor.finish()?;
                return Ok(PlayerModifierNode::AdditionalLandPlay {
                    player: cursor.require_value(player, "an additional-land player")?,
                    count: cursor.require_value(count, "an additional-land count")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "additional_land_play_tests.rs"]
mod tests;
