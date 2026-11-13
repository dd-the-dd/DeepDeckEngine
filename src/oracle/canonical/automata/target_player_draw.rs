use super::super::{MachineResult, OracleTokenCursor};
use super::{
    NumericAtom, TargetListNode, recognize_card_noun, recognize_positive_card_count,
    recognize_target_list,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetPlayerDrawNode {
    pub target: TargetListNode,
    pub count: NumericAtom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetPlayerDrawState {
    Target,
    Action,
    Count,
    Cards,
    Terminator,
    Complete,
}

/// Recognizes the spell envelope `Target player draws N cards.` while leaving
/// the target binding to the lowering layer and the numeric atom to the shared
/// count child.
pub(in crate::oracle::canonical) fn recognize_target_player_draw(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetPlayerDrawNode> {
    let mut state = TargetPlayerDrawState::Target;
    let mut target = None;
    let mut count = None;
    loop {
        state = match state {
            TargetPlayerDrawState::Target => {
                cursor.enter("target-player-draw.target");
                target = Some(recognize_target_list(cursor)?);
                TargetPlayerDrawState::Action
            }
            TargetPlayerDrawState::Action => {
                cursor.enter("target-player-draw.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("draws"),
                    "the singular-subject `draws` action",
                )?;
                TargetPlayerDrawState::Count
            }
            TargetPlayerDrawState::Count => {
                cursor.enter("target-player-draw.count");
                count = Some(recognize_positive_card_count(cursor)?);
                TargetPlayerDrawState::Cards
            }
            TargetPlayerDrawState::Cards => {
                cursor.enter("target-player-draw.cards");
                recognize_card_noun(
                    cursor,
                    count
                        .as_ref()
                        .ok_or_else(|| cursor.fail("a populated draw count"))?,
                )?;
                TargetPlayerDrawState::Terminator
            }
            TargetPlayerDrawState::Terminator => {
                cursor.enter("target-player-draw.terminator");
                cursor.consume(".")?;
                TargetPlayerDrawState::Complete
            }
            TargetPlayerDrawState::Complete => {
                cursor.enter("target-player-draw.complete");
                cursor.finish()?;
                return Ok(TargetPlayerDrawNode {
                    target: cursor.require_value(target, "a populated target player")?,
                    count: cursor.require_value(count, "a populated draw count")?,
                });
            }
        };
    }
}
