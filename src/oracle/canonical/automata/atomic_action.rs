use super::super::super::{MachineResult, OracleTokenCursor};
pub(in crate::oracle::canonical::automata) use crate::oracle::canonical::automata::ActingSubject;
use crate::oracle::canonical::automata::{ArgumentFrame, VerbLemma, recognize_operand_verb};

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

/// One action head of the verb lexicon whose operand grammar is supplied by
/// the parent: the shared clause reads the lemma in the acting subject's
/// agreement (`sacrifice` / `sacrifices`, `discard` / `discards` ...) and
/// stops after the verb. Whether the action is a cost or an effect is the
/// parent's choice; no verb word is tested here.
pub(in crate::oracle::canonical::automata) fn recognize_operand_action<T>(
    cursor: &mut OracleTokenCursor<'_>,
    lemma: &'static [VerbLemma],
    subject: ActingSubject,
    states: [&'static str; 3],
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<T> {
    let [verb_state, operand_state, complete_state] = states;
    cursor.enter(verb_state);
    let verb = recognize_operand_verb(
        cursor,
        subject.agreement,
        lemma,
        &[ArgumentFrame::ObjectSelection],
    )?;
    if verb.is_none() {
        return Err(cursor.fail(&format!(
            "the `{}` action",
            crate::oracle::canonical::automata::verb_entry(lemma[0])
                .surface(subject.agreement.form())
        )));
    }
    cursor.enter(operand_state);
    let operand = recognize_operand(cursor)?;
    cursor.enter(complete_state);
    Ok(operand)
}

/// Recognizes the reusable action head `sacrifice OPERAND` while the active
/// parent supplies the operand grammar and decides whether the result becomes
/// a cost or an effect. No semantic cost/effect distinction is hidden here.
pub(in crate::oracle::canonical::automata) fn recognize_sacrifice_action<T>(
    cursor: &mut OracleTokenCursor<'_>,
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<SacrificeActionNode<T>> {
    recognize_sacrifice_action_with_form(cursor, ActingSubject::CONTROLLER, recognize_operand)
}

/// Same action head with the verb agreement of the parent's subject.
pub(in crate::oracle::canonical::automata) fn recognize_sacrifice_action_with_form<T>(
    cursor: &mut OracleTokenCursor<'_>,
    subject: ActingSubject,
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<SacrificeActionNode<T>> {
    recognize_operand_action(
        cursor,
        &[VerbLemma::Sacrifice],
        subject,
        [
            "action.sacrifice.verb",
            "action.sacrifice.operand",
            "action.sacrifice.complete",
        ],
        recognize_operand,
    )
    .map(|operand| SacrificeActionNode { operand })
}

/// Recognizes only the reusable discard action head and its parent-supplied
/// operand. The parent retains ownership of the actor, quantity, selection
/// policy, and whether the resulting action is a cost or an effect.
pub(in crate::oracle::canonical::automata) fn recognize_discard_action<T>(
    cursor: &mut OracleTokenCursor<'_>,
    subject: ActingSubject,
    recognize_operand: impl FnOnce(&mut OracleTokenCursor<'_>) -> MachineResult<T>,
) -> MachineResult<DiscardActionNode<T>> {
    recognize_operand_action(
        cursor,
        &[VerbLemma::Discard],
        subject,
        [
            "action.discard.verb",
            "action.discard.operand",
            "action.discard.complete",
        ],
        recognize_operand,
    )
    .map(|operand| DiscardActionNode { operand })
}

#[cfg(test)]
#[path = "atomic_action_tests.rs"]
mod tests;
