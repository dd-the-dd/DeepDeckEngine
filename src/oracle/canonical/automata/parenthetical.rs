use crate::oracle::model::RecognizedEntity;

use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug)]
pub(in crate::oracle::canonical) struct ParentheticalAbilityEnvelope<'a> {
    pub inner: &'a [RecognizedEntity],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParentheticalEnvelopeState {
    Open,
    Body,
    Close,
    Complete,
}

/// Resolves a whole-ability parenthetical envelope without rebuilding or
/// trimming Oracle text. The child receives the original inner tokens and
/// therefore preserves their lexical classes and source spans.
pub(in crate::oracle::canonical) fn recognize_parenthetical_ability_envelope<'a>(
    tokens: &'a [RecognizedEntity],
) -> MachineResult<ParentheticalAbilityEnvelope<'a>> {
    let mut cursor = OracleTokenCursor::new("parenthetical-ability-envelope", tokens);
    let mut state = ParentheticalEnvelopeState::Open;
    let mut inner_end = None;
    loop {
        state = match state {
            ParentheticalEnvelopeState::Open => {
                cursor.enter("envelope.parenthetical.open");
                cursor.consume("(")?;
                ParentheticalEnvelopeState::Body
            }
            ParentheticalEnvelopeState::Body => {
                cursor.enter("envelope.parenthetical.body");
                let close = tokens.len().checked_sub(1).ok_or_else(|| {
                    cursor.fail("a nonempty ability followed by a closing parenthesis")
                })?;
                cursor.require(
                    close > cursor.position(),
                    "a nonempty parenthetical ability",
                )?;
                inner_end = Some(close);
                cursor.advance_to(close)?;
                ParentheticalEnvelopeState::Close
            }
            ParentheticalEnvelopeState::Close => {
                cursor.enter("envelope.parenthetical.close");
                cursor.consume(")")?;
                ParentheticalEnvelopeState::Complete
            }
            ParentheticalEnvelopeState::Complete => {
                cursor.enter("envelope.parenthetical.complete");
                cursor.finish()?;
                let end = cursor.require_value(inner_end, "an inner token boundary")?;
                return Ok(ParentheticalAbilityEnvelope {
                    inner: &tokens[1..end],
                });
            }
        };
    }
}
