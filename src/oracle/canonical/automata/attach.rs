use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// Verb frame `attach OBJECT to OBJECT` (CR 701.3). Both slots are the shared
/// permanent operand, so the attached object and its destination are a source
/// reference, an anaphor bound earlier in the sequence, or a target list. The
/// engine owns the legality of the attachment (CR 301.5c, 701.3b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AttachEffectNode {
    pub attachment: PermanentOperandNode,
    pub recipient: PermanentOperandNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Action,
    Attachment,
    Preposition,
    Recipient,
    Complete,
}

/// Neither slot may be a collection: `each`/`all` quantification would need a
/// per-object attachment program that is not represented.
fn require_individual_operand(
    cursor: &OracleTokenCursor<'_>,
    operand: &PermanentOperandNode,
) -> MachineResult<()> {
    cursor.require(
        !matches!(operand, PermanentOperandNode::Collection(_)),
        "an individual attached object or destination, not a quantified collection",
    )
}

pub(in crate::oracle::canonical) fn recognize_attach_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<AttachEffectNode> {
    let mut state = State::Action;
    let mut attachment = None;
    let mut recipient = None;
    loop {
        state = match state {
            State::Action => {
                cursor.enter("effect.attach.action");
                cursor.consume("attach")?;
                State::Attachment
            }
            State::Attachment => {
                cursor.enter("effect.attach.attachment");
                let operand = recognize_contextual_permanent_operand(
                    cursor,
                    name_links,
                    name_failure,
                    context,
                )?;
                require_individual_operand(cursor, &operand)?;
                attachment = Some(operand);
                State::Preposition
            }
            State::Preposition => {
                cursor.enter("effect.attach.preposition");
                cursor.consume("to")?;
                State::Recipient
            }
            State::Recipient => {
                cursor.enter("effect.attach.recipient");
                let operand = recognize_contextual_permanent_operand(
                    cursor,
                    name_links,
                    name_failure,
                    context,
                )?;
                require_individual_operand(cursor, &operand)?;
                if let PermanentOperandNode::Targets(targets) = &operand {
                    cursor.require(
                        targets
                            .slots
                            .iter()
                            .all(|slot| slot.cardinality.maximum <= 1),
                        "a single attachment destination",
                    )?;
                }
                recipient = Some(operand);
                State::Complete
            }
            State::Complete => {
                cursor.enter("effect.attach.complete");
                return Ok(AttachEffectNode {
                    attachment: cursor.require_value(attachment, "the attached object")?,
                    recipient: cursor.require_value(recipient, "the attachment destination")?,
                });
            }
        };
    }
}
