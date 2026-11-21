//! Numeric facts about the ability controller's game state, as a shared
//! numeric child (for example a postposed `where X is` definition).
//!
//! ```text
//! PlayerFact := "the" "amount" "of" "life" LifeClause[past, gapped amount]
//!             | "your" "life" "total"
//! ```
//!
//! `the amount of life you gained this turn` is the relative clause of the
//! shared life clause whose amount slot the head noun fills.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ClauseAction, ClauseAmountPolicy, ClauseFrame, ClauseModality, ClauseSubject,
    ClauseSubjectPolicy, ClauseTense, EventActorScope, LIFE_LEMMAS, LocalPlayerReference,
    NumericExpression, PlayerNumericFact, TargetPlayerLifeAction, clause_starts, recognize_clause,
};

const AMOUNT_OF_LIFE: &[&str] = &["the", "amount", "of", "life"];
const LIFE_TOTAL: &[&str] = &["your", "life", "total"];

fn phrase_starts(cursor: &OracleTokenCursor<'_>, phrase: &[&str]) -> bool {
    phrase.iter().enumerate().all(|(i, word)| {
        cursor
            .peek_ahead(i)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    })
}

fn relative_life_frame() -> ClauseFrame<'static> {
    ClauseFrame {
        subject: ClauseSubjectPolicy::PlayerScope,
        modality: ClauseModality::Indicative,
        tense: ClauseTense::PastThisTurn,
        amount: ClauseAmountPolicy::Gapped,
        lemmas: LIFE_LEMMAS,
    }
}

fn amount_of_life_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if !phrase_starts(cursor, AMOUNT_OF_LIFE) {
        return false;
    }
    let mut relative = cursor.clone();
    relative
        .advance_to(relative.position() + AMOUNT_OF_LIFE.len())
        .is_ok()
        && clause_starts(&relative, &relative_life_frame())
}

pub(in crate::oracle::canonical) fn player_numeric_fact_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    amount_of_life_starts(cursor) || phrase_starts(cursor, LIFE_TOTAL)
}

pub(in crate::oracle::canonical) fn recognize_player_numeric_fact(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.player-fact.start");
    let fact = if phrase_starts(cursor, AMOUNT_OF_LIFE) {
        cursor.consume_phrase(AMOUNT_OF_LIFE)?;
        cursor.enter("numeric.player-fact.relative-life-clause");
        let clause = recognize_clause(cursor, &relative_life_frame())
            .into_result()?
            .ok_or_else(|| cursor.fail("a past life clause after `the amount of life`"))?;
        let ClauseAction::LifeTotalChange { direction, .. } = clause.action;
        cursor.require(
            clause.subject
                == ClauseSubject::Scope {
                    actor: EventActorScope::Controller,
                    one_or_more: false,
                }
                && direction == TargetPlayerLifeAction::Gain,
            "a recorded life amount (`the amount of life you gained this turn`)",
        )?;
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
