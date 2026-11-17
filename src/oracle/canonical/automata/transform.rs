use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{TransformPermanentEffectNode, recognize_source_reference};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransformState {
    Action,
    Object,
    Complete,
}

/// Recognizes the atomic transform action for the current source object.
///
/// Double-faced eligibility and the CR 701.27g once-per-stack guard belong to
/// runtime validation. This machine only resolves the grammatical source
/// reference and never infers eligibility from Oracle prose.
pub(in crate::oracle::canonical) fn recognize_transform_source_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<TransformPermanentEffectNode> {
    let mut state = TransformState::Action;
    let mut permanent = None;
    loop {
        state = match state {
            TransformState::Action => {
                cursor.enter("effect.transform.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("transform"),
                    "the `transform` action",
                )?;
                TransformState::Object
            }
            TransformState::Object => {
                cursor.enter("effect.transform.object");
                permanent = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                TransformState::Complete
            }
            TransformState::Complete => {
                cursor.enter("effect.transform.complete");
                return Ok(TransformPermanentEffectNode {
                    permanent: cursor.require_value(permanent, "a populated transform source")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
