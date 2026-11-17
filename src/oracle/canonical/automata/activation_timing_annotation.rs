//! Parenthesized activation explanations are typed metadata, never effects.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationTimingAnnotation {
    InstantTimingExplanation,
}

#[derive(Clone, Copy)]
enum State {
    Open,
    Action,
    Restriction,
    Relation,
    Article,
    Timing,
    Period,
    Close,
    Complete,
}

/// The parent must prove that the resulting ability uses the stack. A mana
/// ability cannot acquire instant timing merely by discarding this annotation.
pub(in crate::oracle::canonical) fn recognize_activation_timing_annotation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationTimingAnnotation> {
    let mut state = State::Open;
    loop {
        state = match state {
            State::Open => {
                cursor.enter("activation.annotation.open");
                cursor.consume("(")?;
                State::Action
            }
            State::Action => {
                cursor.enter("activation.annotation.action");
                cursor.consume("activate")?;
                State::Restriction
            }
            State::Restriction => {
                cursor.enter("activation.annotation.restriction");
                cursor.consume("only")?;
                State::Relation
            }
            State::Relation => {
                cursor.enter("activation.annotation.relation");
                cursor.consume("as")?;
                State::Article
            }
            State::Article => {
                cursor.enter("activation.annotation.article");
                cursor.consume("an")?;
                State::Timing
            }
            State::Timing => {
                cursor.enter("activation.annotation.timing");
                cursor.consume("instant")?;
                State::Period
            }
            State::Period => {
                cursor.enter("activation.annotation.period");
                cursor.consume(".")?;
                State::Close
            }
            State::Close => {
                cursor.enter("activation.annotation.close");
                cursor.consume(")")?;
                State::Complete
            }
            State::Complete => return Ok(ActivationTimingAnnotation::InstantTimingExplanation),
        }
    }
}
