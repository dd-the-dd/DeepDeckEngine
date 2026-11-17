//! Numeric facts about the ability controller's game state, as a shared
//! numeric child (for example a postposed `where X is` definition).
//!
//! ```text
//! PlayerFact := "the" "amount" "of" "life" "you" "gained" "this" "turn"
//!             | "your" "life" "total"
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalPlayerReference, NumericExpression, PlayerNumericFact};

const LIFE_GAINED: &[&str] = &[
    "the", "amount", "of", "life", "you", "gained", "this", "turn",
];
const LIFE_TOTAL: &[&str] = &["your", "life", "total"];

fn phrase_starts(cursor: &OracleTokenCursor<'_>, phrase: &[&str]) -> bool {
    phrase.iter().enumerate().all(|(i, word)| {
        cursor
            .peek_ahead(i)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    })
}

pub(in crate::oracle::canonical) fn player_numeric_fact_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    phrase_starts(cursor, LIFE_GAINED) || phrase_starts(cursor, LIFE_TOTAL)
}

pub(in crate::oracle::canonical) fn recognize_player_numeric_fact(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.player-fact.start");
    let fact = if phrase_starts(cursor, LIFE_GAINED) {
        cursor.consume_phrase(LIFE_GAINED)?;
        PlayerNumericFact::LifeGainedThisTurn
    } else {
        cursor.consume_phrase(LIFE_TOTAL)?;
        PlayerNumericFact::LifeTotal
    };
    cursor.enter("numeric.player-fact.complete");
    Ok(NumericExpression::PlayerFact {
        player: LocalPlayerReference::Controller,
        fact,
    })
}
