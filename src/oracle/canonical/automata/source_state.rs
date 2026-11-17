use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{LocalObjectReference, recognize_source_reference_with_antecedent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceStateChange {
    Prepared,
    Tapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceStateEffectNode {
    pub object: LocalObjectReference,
    pub state: SourceStateChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceStateEffectState {
    Source,
    Action,
    State,
    Terminator,
    Complete,
}

/// Recognizes the independent atomic effect `SOURCE becomes STATE.`. Source
/// names, `this TYPE`, and bound source anaphors share the normal reference
/// resolver; the state vocabulary remains typed.
pub(in crate::oracle::canonical) fn recognize_source_state_effect(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    antecedent: Option<&LocalObjectReference>,
) -> MachineResult<SourceStateEffectNode> {
    let mut machine_state = SourceStateEffectState::Source;
    let mut object = None;
    let mut state = None;
    loop {
        machine_state = match machine_state {
            SourceStateEffectState::Source => {
                cursor.enter("effect.source-state.source");
                object = Some(recognize_source_reference_with_antecedent(
                    cursor,
                    name_links,
                    name_failure,
                    antecedent,
                )?);
                SourceStateEffectState::Action
            }
            SourceStateEffectState::Action => {
                cursor.enter("effect.source-state.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("becomes"),
                    "the `becomes` state transition",
                )?;
                SourceStateEffectState::State
            }
            SourceStateEffectState::State => {
                cursor.enter("effect.source-state.state");
                let token = cursor.consume_type("stateWord")?;
                state = Some(if token.raw.eq_ignore_ascii_case("prepared") {
                    SourceStateChange::Prepared
                } else if token.raw.eq_ignore_ascii_case("tapped") {
                    SourceStateChange::Tapped
                } else {
                    return Err(cursor.fail("the `prepared` or `tapped` state"));
                });
                SourceStateEffectState::Terminator
            }
            SourceStateEffectState::Terminator => {
                cursor.enter("effect.source-state.terminator");
                cursor.consume(".")?;
                SourceStateEffectState::Complete
            }
            SourceStateEffectState::Complete => {
                cursor.enter("effect.source-state.complete");
                return Ok(SourceStateEffectNode {
                    object: cursor.require_value(object, "a populated source object")?,
                    state: cursor.require_value(state, "a populated source state")?,
                });
            }
        };
    }
}
