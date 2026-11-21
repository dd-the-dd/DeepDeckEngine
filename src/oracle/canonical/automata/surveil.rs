use super::super::{MachineResult, OracleTokenCursor};
use super::{LibraryLookAction, recognize_library_look_reminder};
use super::{
    LocalPlayerReference, NumericAtom, NumericExpression, SurveilEffectNode, TargetBinding,
    recognize_numeric_expression,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurveilState {
    Action,
    Count,
    Complete,
}

/// Recognizes the atomic keyword action `surveil COUNT`.
///
/// Trigger envelopes, punctuation, and reminder text remain parent-owned. The
/// count delegates to the shared numeric machine so all effects retain the
/// same typed representation for literal and variable quantities.
pub(in crate::oracle::canonical) fn recognize_surveil_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<SurveilEffectNode> {
    let mut state = SurveilState::Action;
    let mut count = None;
    loop {
        state = match state {
            SurveilState::Action => {
                cursor.enter("effect.surveil.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("surveil"),
                    "the `surveil` action",
                )?;
                SurveilState::Count
            }
            SurveilState::Count => {
                cursor.enter("effect.surveil.count");
                let expression = recognize_numeric_expression(cursor, antecedent)?;
                cursor.require(
                    !matches!(
                        &expression,
                        NumericExpression::Atom(NumericAtom::Integer(value)) if *value <= 0
                    ),
                    "a positive surveil count or variable expression",
                )?;
                count = Some(expression);
                SurveilState::Complete
            }
            SurveilState::Complete => {
                cursor.enter("effect.surveil.complete");
                return Ok(SurveilEffectNode {
                    player: LocalPlayerReference::Controller,
                    count: cursor.require_value(count, "a surveil count")?,
                });
            }
        };
    }
}

/// Validates the canonical reminder of a literal `surveil N` action with the
/// same reader as scry; only the verb and the destination wording differ.
pub(in crate::oracle::canonical) fn recognize_surveil_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericExpression,
) -> MachineResult<()> {
    recognize_library_look_reminder(cursor, LibraryLookAction::Surveil, count)
}

#[cfg(test)]
#[path = "surveil_tests.rs"]
mod tests;
