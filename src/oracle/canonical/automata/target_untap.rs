use super::super::{MachineResult, OracleTokenCursor};
use super::{RecognizedEffectFragment, TargetBinding};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct UntapTargetEffectNode {
    pub(in crate::oracle::canonical) permanent: TargetBinding,
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
    let mut state = State::Action;
    let mut permanent = None;
    loop {
        state = match state {
            State::Action => {
                cursor.enter("effect.untap-target.action");
                cursor.consume("untap")?;
                State::Reference
            }
            State::Reference => {
                cursor.enter("effect.untap-target.reference");
                permanent = Some(
                    antecedent.ok_or_else(|| cursor.fail("a preceding singular target binding"))?,
                );
                cursor.consume("it")?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("effect.untap-target.complete");
                return Ok(RecognizedEffectFragment {
                    node: UntapTargetEffectNode {
                        permanent: cursor
                            .require_value(permanent, "the referenced target binding")?,
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
