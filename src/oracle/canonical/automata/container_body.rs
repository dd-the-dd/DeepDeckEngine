//! The one body slot of every typed container (S6).
//!
//! A container owns only its structural markers: a Saga chapter its Roman
//! numerals and dash (CR 715.2), a modal ability its header and bullets (CR
//! 700.2), a band its threshold (CR 711.2, 722.2, 702.169), a quoted ability
//! its quotation marks (CR 113.1). After those markers commit, the body is
//! read here and nowhere else: printed modes when the modal container's own
//! markers are present, otherwise the shared effect program. A container
//! never keeps a private list of accepted bodies.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, TriggerBodyNode, modal_body_starts,
    recognize_effect_program_with_context, recognize_effect_sequence_modal_with_context,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// Reads one container body. `program_context` is the context of the
/// container's effect program; `modal_context` the one its printed modes are
/// read with (sentence frames inside a mode belong to the modal lowering).
pub(in crate::oracle::canonical) fn recognize_container_body(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    program_context: &EffectRecognitionContext,
    modal_context: &EffectRecognitionContext,
) -> MachineResult<TriggerBodyNode> {
    cursor.enter("container.body");
    if modal_body_starts(cursor) {
        cursor.enter("container.body.modal");
        return recognize_effect_sequence_modal_with_context(
            cursor,
            name_links,
            name_failure,
            modal_context,
        )
        .map(TriggerBodyNode::Modal);
    }
    cursor.enter("container.body.program");
    recognize_effect_program_with_context(cursor, name_links, name_failure, program_context)
        .map(TriggerBodyNode::Program)
}
