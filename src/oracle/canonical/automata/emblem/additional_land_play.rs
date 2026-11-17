use super::super::super::{MachineResult, OracleTokenCursor};
use super::super::{LocalPlayerReference, recognize_additional_land_allowance_header};
use super::PlayerModifierNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdditionalLandPlayState {
    PeriodOn,
    PeriodEach,
    PeriodOf,
    PeriodPlayer,
    PeriodTurns,
    Terminator,
    Complete,
}

/// Recognizes a persistent player modifier of the form
/// `PLAYER may play QUANTITY additional land(s) on each of PLAYER's turns`.
/// The static parent currently accepts only the ability controller, but the
/// player and numeric children remain typed so other parents can reuse them.
pub(in crate::oracle::canonical) fn recognize_static_additional_land_play(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerModifierNode> {
    let modifier = recognize_additional_land_allowance_header(cursor)?;
    let PlayerModifierNode::AdditionalLandPlay { player, .. } = &modifier else {
        return Err(cursor.fail("an additional-land allowance"));
    };
    let mut state = AdditionalLandPlayState::PeriodOn;
    loop {
        state = match state {
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
                    *player == LocalPlayerReference::Controller,
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
                return Ok(modifier);
            }
        };
    }
}

#[cfg(test)]
#[path = "additional_land_play_tests.rs"]
mod tests;
