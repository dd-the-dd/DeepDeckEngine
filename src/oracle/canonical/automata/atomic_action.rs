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
    /// The base verb for a plural third-person payer (`unless they sacrifice
    /// ... of their choice`): imperative surface, third-person chooser marker.
    ThirdPersonPlural,
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
    recognize_sacrifice_action_with_form(cursor, DiscardVerbForm::Imperative, recognize_operand)
}

/// Same action head with the verb conjugation chosen by the parent's subject:
/// imperative for the ability controller, third person for another actor.
/// `DiscardVerbForm` names the shared conjugation of every action verb.
pub(in crate::oracle::canonical::automata) fn recognize_sacrifice_action_with_form<T>(
    cursor: &mut OracleTokenCursor<'_>,
    verb_form: DiscardVerbForm,
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<SacrificeActionNode<T>> {
    let mut state = SacrificeActionState::Verb;
    let mut recognize_operand = Some(recognize_operand);
    let mut operand = None;
    loop {
        state = match state {
            SacrificeActionState::Verb => {
                cursor.enter("action.sacrifice.verb");
                let action_index = cursor.position();
                // The shared lexicon types only the imperative `sacrifice` as
                // an action word; the third-person form is read by its raw word.
                let action = match verb_form {
                    DiscardVerbForm::Imperative | DiscardVerbForm::ThirdPersonPlural => {
                        cursor.consume_type("actionWord")?.clone()
                    }
                    DiscardVerbForm::ThirdPersonSingular => {
                        let token = cursor
                            .peek_token()
                            .cloned()
                            .ok_or_else(|| cursor.fail("the `sacrifices` action"))?;
                        if token.raw.eq_ignore_ascii_case("sacrifices") {
                            cursor.advance_to(action_index + 1)?;
                        }
                        token
                    }
                };
                let expected = match verb_form {
                    DiscardVerbForm::Imperative | DiscardVerbForm::ThirdPersonPlural => "sacrifice",
                    DiscardVerbForm::ThirdPersonSingular => "sacrifices",
                };
                if !action.raw.eq_ignore_ascii_case(expected) {
                    return Err(cursor.fail_with_span(
                        &format!("the `{expected}` action"),
                        action_index,
                        action.span_start,
                        action.span_end,
                        action.raw.as_str(),
                    ));
                }
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
                    DiscardVerbForm::Imperative | DiscardVerbForm::ThirdPersonPlural => "discard",
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

/// Imperative action heads whose operand grammar is supplied by the parent,
/// like `sacrifice` and `discard`: `tap`, `exile`, `remove` (counters) and
/// the keyword action `blight`. Whether the action is a cost or an effect is the parent's choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical::automata) enum ImperativeActionVerb {
    Tap,
    Exile,
    Blight,
    Remove,
}

impl ImperativeActionVerb {
    fn word(self) -> &'static str {
        match self {
            Self::Tap => "tap",
            Self::Exile => "exile",
            Self::Blight => "blight",
            Self::Remove => "remove",
        }
    }

    fn states(self) -> [&'static str; 3] {
        match self {
            Self::Tap => [
                "action.tap.verb",
                "action.tap.operand",
                "action.tap.complete",
            ],
            Self::Exile => [
                "action.exile.verb",
                "action.exile.operand",
                "action.exile.complete",
            ],
            Self::Blight => [
                "action.blight.verb",
                "action.blight.operand",
                "action.blight.complete",
            ],
            Self::Remove => [
                "action.remove.verb",
                "action.remove.operand",
                "action.remove.complete",
            ],
        }
    }
}

/// Recognizes one action head and its parent-supplied operand, the verb
/// conjugated for the parent's subject: the imperative for the ability
/// controller, the third person (`taps`, `exiles`…) for another player.
pub(in crate::oracle::canonical::automata) fn recognize_action_with_form<T>(
    cursor: &mut OracleTokenCursor<'_>,
    verb: ImperativeActionVerb,
    verb_form: DiscardVerbForm,
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<T> {
    let [verb_state, operand_state, complete_state] = verb.states();
    cursor.enter(verb_state);
    match verb_form {
        DiscardVerbForm::Imperative | DiscardVerbForm::ThirdPersonPlural => {
            cursor.consume(verb.word())?
        }
        DiscardVerbForm::ThirdPersonSingular => cursor.consume(&format!("{}s", verb.word()))?,
    };
    cursor.enter(operand_state);
    let operand = recognize_operand(cursor)?;
    cursor.enter(complete_state);
    Ok(operand)
}

#[cfg(test)]
#[path = "atomic_action_tests.rs"]
mod tests;
