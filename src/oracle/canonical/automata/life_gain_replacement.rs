//! Life-gain replacement frame (CR 614.1a, 119.10 a player gains life).
//!
//! ```text
//! Frame   := "if" Player "would" "gain" "life" "," Subject Gain Amount "instead" "."
//! Player  := "you" | "an" "opponent" | "a" "player"
//! Subject := "you" (for `you`) | "that" "player" (otherwise)
//! Gain    := "gain" | "gains"
//! Amount  := the shared event-quantity amount over `life`
//! ```
//!
//! The event names the affected player relation; the replacing action is the
//! same life gain with a new amount, so its subject must restate that player.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{NumericExpression, recognize_event_quantity_expression};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LifeGainPlayer {
    You,
    Opponent,
    AnyPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LifeGainReplacementNode {
    pub player: LifeGainPlayer,
    pub amount: NumericExpression,
}

pub(in crate::oracle::canonical) enum LifeGainReplacementRecognition {
    Parsed(LifeGainReplacementNode),
    NotClaimed,
    Failed(MachineFailure),
}

fn peek_is(cursor: &OracleTokenCursor<'_>, word: &str) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// `would gain life` after a player subject commits this frame.
pub(in crate::oracle::canonical) fn try_recognize_life_gain_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> LifeGainReplacementRecognition {
    let checkpoint = cursor.checkpoint();
    let mut committed = false;
    match recognize_life_gain_replacement(cursor, &mut committed) {
        Ok(node) => LifeGainReplacementRecognition::Parsed(node),
        Err(failure) if committed => LifeGainReplacementRecognition::Failed(failure),
        Err(_) => {
            cursor.rollback(checkpoint);
            LifeGainReplacementRecognition::NotClaimed
        }
    }
}

fn recognize_life_gain_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    committed: &mut bool,
) -> MachineResult<LifeGainReplacementNode> {
    cursor.enter("replacement.life-gain.envelope");
    cursor.consume("if")?;
    cursor.enter("replacement.life-gain.player");
    let player = if peek_is(cursor, "you") {
        cursor.consume("you")?;
        LifeGainPlayer::You
    } else if peek_is(cursor, "an") {
        cursor.consume_phrase(&["an", "opponent"])?;
        LifeGainPlayer::Opponent
    } else {
        cursor.consume_phrase(&["a", "player"])?;
        LifeGainPlayer::AnyPlayer
    };
    cursor.enter("replacement.life-gain.event");
    cursor.consume_phrase(&["would", "gain", "life"])?;
    *committed = true;
    cursor.enter("replacement.life-gain.separator");
    cursor.consume(",")?;
    cursor.enter("replacement.life-gain.subject");
    if player == LifeGainPlayer::You {
        cursor.consume_phrase(&["you", "gain"])?;
    } else {
        cursor.consume_phrase(&["that", "player", "gains"])?;
    }
    let amount = recognize_event_quantity_expression(cursor, "life")?;
    cursor.enter("replacement.life-gain.instead");
    cursor.consume("instead")?;
    cursor.consume(".")?;
    cursor.enter("replacement.life-gain.complete");
    cursor.finish()?;
    Ok(LifeGainReplacementNode { player, amount })
}
