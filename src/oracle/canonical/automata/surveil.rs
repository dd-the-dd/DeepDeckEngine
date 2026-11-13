use super::super::{MachineResult, OracleTokenCursor};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurveilReminderState {
    Open,
    Look,
    Cards,
    Destination,
    Close,
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

/// Recognizes the canonical reminder attached to a literal `surveil N` atom.
/// The effect node remains independent; an ability envelope opts into this
/// child only when a parenthetical suffix is present.
pub(in crate::oracle::canonical) fn recognize_surveil_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericExpression,
) -> MachineResult<()> {
    let expected_count = match count {
        NumericExpression::Atom(NumericAtom::Integer(value)) if *value > 0 => *value,
        _ => {
            return Err(cursor.fail("a literal positive surveil count for reminder validation"));
        }
    };
    let mut state = SurveilReminderState::Open;
    loop {
        state = match state {
            SurveilReminderState::Open => {
                cursor.enter("effect.surveil.reminder.open");
                cursor.consume("(")?;
                SurveilReminderState::Look
            }
            SurveilReminderState::Look => {
                cursor.enter("effect.surveil.reminder.look");
                cursor.consume_phrase(&["look", "at", "the", "top"])?;
                SurveilReminderState::Cards
            }
            SurveilReminderState::Cards => {
                cursor.enter("effect.surveil.reminder.cards");
                if expected_count == 1 {
                    cursor.consume("card")?;
                } else {
                    let actual = recognize_numeric_expression(cursor, None)?;
                    cursor.require(
                        actual == NumericExpression::Atom(NumericAtom::Integer(expected_count)),
                        "the same literal count as the surveil effect",
                    )?;
                    cursor.consume("cards")?;
                }
                cursor.consume_phrase(&["of", "your", "library"])?;
                SurveilReminderState::Destination
            }
            SurveilReminderState::Destination => {
                cursor.enter("effect.surveil.reminder.destination");
                if expected_count == 1 {
                    cursor.consume_phrase(&[
                        ".",
                        "you",
                        "may",
                        "put",
                        "it",
                        "into",
                        "your",
                        "graveyard",
                        ".",
                    ])?;
                } else {
                    cursor.consume_phrase(&[
                        ",",
                        "then",
                        "put",
                        "any",
                        "number",
                        "of",
                        "them",
                        "into",
                        "your",
                        "graveyard",
                        "and",
                        "the",
                        "rest",
                        "on",
                        "top",
                        "of",
                        "your",
                        "library",
                        "in",
                        "any",
                        "order",
                        ".",
                    ])?;
                }
                SurveilReminderState::Close
            }
            SurveilReminderState::Close => {
                cursor.enter("effect.surveil.reminder.close");
                cursor.consume(")")?;
                SurveilReminderState::Complete
            }
            SurveilReminderState::Complete => {
                cursor.enter("effect.surveil.reminder.complete");
                return Ok(());
            }
        };
    }
}

#[cfg(test)]
#[path = "surveil_tests.rs"]
mod tests;
