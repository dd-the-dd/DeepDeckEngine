//! Atomic turn quotas compose a player population, amount and spell query.
use super::super::{MachineFailure, OracleTokenCursor};
use super::{
    NumericExpression, PlayerSetReferenceNode, SpellCastQueryNode, recognize_numeric_expression,
    recognize_player_set_reference, recognize_spell_cast_collection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CastQuotaNode {
    pub players: PlayerSetReferenceNode,
    pub maximum: NumericExpression,
    pub spells: SpellCastQueryNode,
}

pub(in crate::oracle::canonical) enum CastQuotaRecognition {
    Parsed(CastQuotaNode),
    NotClaimed,
    UnsupportedEnvelope(MachineFailure),
    Failed(MachineFailure),
}

#[derive(Clone, Copy)]
enum State {
    Players,
    Prohibition,
    Action,
    More,
    Comparison,
    Maximum,
    Spells,
    Period,
    Turn,
    Terminator,
    Complete,
}
#[derive(Default)]
struct Frame {
    players: Option<PlayerSetReferenceNode>,
    maximum: Option<NumericExpression>,
    spells: Option<SpellCastQueryNode>,
    committed: bool,
}

pub(in crate::oracle::canonical) fn try_recognize_cast_quota(
    cursor: &mut OracleTokenCursor<'_>,
) -> CastQuotaRecognition {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each") || raw.eq_ignore_ascii_case("your"))
    {
        return CastQuotaRecognition::NotClaimed;
    }
    let checkpoint = cursor.checkpoint();
    let mut state = State::Players;
    let mut memory = Frame::default();
    let result: Result<CastQuotaNode, MachineFailure> = (|| {
        loop {
            state = match state {
                State::Players => {
                    cursor.enter("static.cast-quota.players");
                    memory.players = Some(recognize_player_set_reference(cursor)?);
                    State::Prohibition
                }
                State::Prohibition => {
                    cursor.enter("static.cast-quota.prohibition");
                    let word = cursor
                        .peek()
                        .filter(|raw| {
                            raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("cannot")
                        })
                        .ok_or_else(|| cursor.fail("can't or cannot"))?
                        .to_owned();
                    cursor.consume(&word)?;
                    State::Action
                }
                State::Action => {
                    cursor.enter("static.cast-quota.action");
                    cursor.consume("cast")?;
                    State::More
                }
                State::More => {
                    cursor.enter("static.cast-quota.maximum-operator");
                    cursor.consume("more")?;
                    memory.committed = true;
                    State::Comparison
                }
                State::Comparison => {
                    cursor.enter("static.cast-quota.comparison");
                    cursor.consume("than")?;
                    State::Maximum
                }
                State::Maximum => {
                    cursor.enter("static.cast-quota.maximum");
                    memory.maximum = Some(recognize_numeric_expression(cursor, None)?);
                    State::Spells
                }
                State::Spells => {
                    cursor.enter("static.cast-quota.spells");
                    memory.spells = Some(recognize_spell_cast_collection(cursor)?);
                    State::Period
                }
                State::Period => {
                    cursor.enter("static.cast-quota.period");
                    cursor.consume("each")?;
                    State::Turn
                }
                State::Turn => {
                    cursor.enter("static.cast-quota.turn");
                    cursor.consume("turn")?;
                    State::Terminator
                }
                State::Terminator => {
                    cursor.enter("static.cast-quota.terminator");
                    cursor.consume(".")?;
                    State::Complete
                }
                State::Complete => {
                    return Ok(CastQuotaNode {
                        players: cursor.require_value(memory.players, "a player population")?,
                        maximum: cursor
                            .require_value(memory.maximum.take(), "a maximum expression")?,
                        spells: cursor.require_value(memory.spells.take(), "a spell query")?,
                    });
                }
            }
        }
    })();
    match result {
        Ok(node) => {
            // A successful prefix is not an atomic-family claim. Conditional
            // or multi-sentence continuations belong to a broader parent.
            if cursor.peek().is_some() {
                let failure =
                    cursor.fail("one complete atomic cast quota without a later sentence");
                cursor.rollback(checkpoint);
                CastQuotaRecognition::UnsupportedEnvelope(failure)
            } else {
                CastQuotaRecognition::Parsed(node)
            }
        }
        Err(_failure) if !memory.committed => {
            cursor.rollback(checkpoint);
            CastQuotaRecognition::NotClaimed
        }
        Err(failure) if matches!(state, State::Period | State::Terminator) => {
            cursor.rollback(checkpoint);
            CastQuotaRecognition::UnsupportedEnvelope(failure)
        }
        Err(failure) => CastQuotaRecognition::Failed(failure),
    }
}

#[cfg(test)]
#[path = "cast_quota_tests.rs"]
mod tests;
