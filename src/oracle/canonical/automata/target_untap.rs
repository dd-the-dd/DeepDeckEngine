use super::super::{MachineResult, OracleTokenCursor};
use super::{PermanentEffectReference, RecognizedEffectFragment, TargetBinding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct UntapTargetEffectNode {
    pub(in crate::oracle::canonical) permanent: PermanentEffectReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Action,
    Reference,
    Complete,
}

/// Recognizes the atomic `untap it` action. The parent sequence supplies the
/// target antecedent and retains ownership of sentence punctuation.
pub(super) fn recognize_untap_target_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<RecognizedEffectFragment<UntapTargetEffectNode>> {
    recognize_untap_permanent_effect(cursor, antecedent.map(PermanentEffectReference::Target))
}

/// The same action grammar accepts an explicitly supplied typed event object;
/// it never manufactures a target slot or rewrites the ability source.
pub(super) fn recognize_untap_permanent_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<PermanentEffectReference>,
) -> MachineResult<RecognizedEffectFragment<UntapTargetEffectNode>> {
    let mut state = State::Action;
    let mut permanent = None;
    loop {
        state = match state {
            State::Action => {
                cursor.enter("effect.untap-target.action");
                crate::oracle::canonical::automata::consume_lemma(
                    cursor,
                    crate::oracle::canonical::automata::VerbLemma::Untap,
                    crate::oracle::canonical::automata::VerbAgreement::Base,
                )?;
                State::Reference
            }
            State::Reference => {
                cursor.enter("effect.untap-target.reference");
                permanent = Some(antecedent.ok_or_else(|| {
                    cursor.fail("a preceding singular target or observed event-object binding")
                })?);
                cursor.consume("it")?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("effect.untap-target.complete");
                return Ok(RecognizedEffectFragment {
                    node: UntapTargetEffectNode {
                        permanent: cursor
                            .require_value(permanent, "the typed permanent reference")?,
                    },
                    end_position: cursor.position(),
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "target_untap_tests.rs"]
mod tests;
