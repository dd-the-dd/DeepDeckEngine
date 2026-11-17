use crate::oracle::{
    characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type},
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalConditionNode, LocalObjectReference, ObjectCardType, ObjectPredicate,
    recognize_controlled_permanent_presence_query_with, recognize_object_query,
    recognize_source_reference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControlledPermanentConditionalEntryNode {
    pub source: LocalObjectReference,
    pub source_type: ObjectCardType,
    pub required_permanent: ObjectPredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Source,
    Event,
    EnteringState,
    Exception,
    Condition,
    Terminator,
    Complete,
}

#[derive(Default)]
struct Frame {
    source: Option<LocalObjectReference>,
    source_type: Option<ObjectCardType>,
    required_permanent: Option<ObjectPredicate>,
}

/// Recognizes `This TYPE enters tapped unless you control a SELECTOR`.
/// The shared controlled-presence child owns the implicit battlefield domain
/// and object query. This parent owns the entry envelope and negation.
pub(in crate::oracle::canonical) fn recognize_controlled_permanent_conditional_entry(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ControlledPermanentConditionalEntryNode> {
    let mut state = State::Source;
    let mut frame = Frame::default();
    loop {
        state = match state {
            State::Source => {
                cursor.enter("replacement.conditional-entry.source");
                let checkpoint = cursor.checkpoint();
                cursor.consume("this")?;
                let token = cursor
                    .peek_token()
                    .ok_or_else(|| cursor.fail("a permanent type after `this`"))?;
                frame.source_type = Some(
                    lookup_permanent_card_type(&token.raw, AcceptedForms::SingularOnly)
                        .ok_or_else(|| cursor.fail("a permanent type after `this`"))?,
                );
                cursor.rollback(checkpoint);
                frame.source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                State::Event
            }
            State::Event => {
                cursor.enter("replacement.conditional-entry.event");
                cursor.consume("enters")?;
                State::EnteringState
            }
            State::EnteringState => {
                cursor.enter("replacement.conditional-entry.entering-state");
                cursor.consume("tapped")?;
                State::Exception
            }
            State::Exception => {
                cursor.enter("replacement.conditional-entry.exception");
                cursor.consume("unless")?;
                State::Condition
            }
            State::Condition => {
                let condition = recognize_controlled_permanent_presence_query_with(
                    cursor,
                    recognize_object_query,
                )?;
                let LocalConditionNode::ControlledPermanentCount(condition) = condition else {
                    return Err(cursor.fail("a controlled permanent presence condition"));
                };
                frame.required_permanent = Some(condition.predicate);
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("replacement.conditional-entry.terminator");
                cursor.consume(".")?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("replacement.conditional-entry.complete");
                cursor.finish()?;
                return Ok(ControlledPermanentConditionalEntryNode {
                    source: cursor.require_value(frame.source, "a populated source reference")?,
                    source_type: cursor
                        .require_value(frame.source_type, "a populated source type")?,
                    required_permanent: cursor.require_value(
                        frame.required_permanent,
                        "a populated permanent selector",
                    )?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "conditional_entry_tests.rs"]
mod tests;
