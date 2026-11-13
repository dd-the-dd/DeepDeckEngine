use super::super::super::{MachineResult, OracleTokenCursor};

#[path = "counted_card.rs"]
pub(in crate::oracle::canonical::automata) mod counted_card;
#[path = "player_discard.rs"]
pub(in crate::oracle::canonical::automata) mod player_discard;
#[path = "player_subject.rs"]
pub(in crate::oracle::canonical::automata) mod player_subject;
#[path = "selection_method.rs"]
pub(in crate::oracle::canonical::automata) mod selection_method;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical::automata) struct SacrificeActionNode<T> {
    pub(in crate::oracle::canonical::automata) operand: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical::automata) struct DiscardActionNode<T> {
    pub(in crate::oracle::canonical::automata) operand: T,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical::automata) enum DiscardVerbForm {
    Imperative,
    ThirdPersonSingular,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SacrificeActionState {
    Verb,
    Operand,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiscardActionState {
    Verb,
    Operand,
    Complete,
}

/// Recognizes the reusable action head `sacrifice OPERAND` while the active
/// parent supplies the operand grammar and decides whether the result becomes
/// a cost or an effect. No semantic cost/effect distinction is hidden here.
pub(in crate::oracle::canonical::automata) fn recognize_sacrifice_action<T>(
    cursor: &mut OracleTokenCursor<'_>,
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<SacrificeActionNode<T>> {
    let mut state = SacrificeActionState::Verb;
    let mut recognize_operand = Some(recognize_operand);
    let mut operand = None;
    loop {
        state = match state {
            SacrificeActionState::Verb => {
                cursor.enter("action.sacrifice.verb");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("sacrifice"),
                    "the `sacrifice` action",
                )?;
                SacrificeActionState::Operand
            }
            SacrificeActionState::Operand => {
                cursor.enter("action.sacrifice.operand");
                let child = recognize_operand
                    .take()
                    .ok_or_else(|| cursor.fail("one parent-supplied sacrifice operand"))?;
                operand = Some(child(cursor)?);
                SacrificeActionState::Complete
            }
            SacrificeActionState::Complete => {
                cursor.enter("action.sacrifice.complete");
                return Ok(SacrificeActionNode {
                    operand: cursor.require_value(operand, "a populated sacrifice operand")?,
                });
            }
        };
    }
}

/// Recognizes only the reusable discard action head and its parent-supplied
/// operand. The parent retains ownership of the actor, quantity, selection
/// policy, and whether the resulting action is a cost or an effect.
pub(in crate::oracle::canonical::automata) fn recognize_discard_action<T>(
    cursor: &mut OracleTokenCursor<'_>,
    verb_form: DiscardVerbForm,
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<DiscardActionNode<T>> {
    let mut state = DiscardActionState::Verb;
    let mut recognize_operand = Some(recognize_operand);
    let mut operand = None;
    loop {
        state = match state {
            DiscardActionState::Verb => {
                cursor.enter("action.discard.verb");
                let action = cursor.consume_type("actionWord")?;
                let expected = match verb_form {
                    DiscardVerbForm::Imperative => "discard",
                    DiscardVerbForm::ThirdPersonSingular => "discards",
                };
                if !action.raw.eq_ignore_ascii_case(expected) {
                    return Err(cursor.fail_with_span(
                        &format!("the `{expected}` action"),
                        action.index,
                        action.span_start,
                        action.span_end,
                        action.raw.clone(),
                    ));
                }
                DiscardActionState::Operand
            }
            DiscardActionState::Operand => {
                cursor.enter("action.discard.operand");
                let child = recognize_operand
                    .take()
                    .ok_or_else(|| cursor.fail("one parent-supplied discard operand"))?;
                operand = Some(child(cursor)?);
                DiscardActionState::Complete
            }
            DiscardActionState::Complete => {
                cursor.enter("action.discard.complete");
                return Ok(DiscardActionNode {
                    operand: cursor.require_value(operand, "a populated discard operand")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "atomic_action_tests.rs"]
mod tests;
