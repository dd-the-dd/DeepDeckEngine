use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ControlledCountDirection, LocalObjectReference, ObjectPredicate,
    recognize_controlled_permanent_count_query, recognize_source_reference,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControlledCountEntryNode {
    pub source: LocalObjectReference,
    pub threshold: i64,
    pub direction: ControlledCountDirection,
    pub predicate: ObjectPredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Source,
    EntryState,
    Unless,
    CountQuery,
    Terminator,
    Complete,
}

#[derive(Default)]
struct AttemptFrame {
    source: Option<LocalObjectReference>,
    threshold: Option<i64>,
    direction: Option<ControlledCountDirection>,
    predicate: Option<ObjectPredicate>,
}

/// Recognizes an entering-state exemption based on how many matching other
/// permanents the source's controller already controls. Quantity, comparison
/// direction and permanent selector remain independent typed values.
pub(in crate::oracle::canonical) fn recognize_controlled_count_entry(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ControlledCountEntryNode> {
    let mut state = State::Source;
    let mut frame = AttemptFrame::default();
    loop {
        state = match state {
            State::Source => {
                cursor.enter("replacement.controlled-count-entry.source");
                frame.source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                State::EntryState
            }
            State::EntryState => {
                cursor.enter("replacement.controlled-count-entry.entry-state");
                cursor.consume_phrase(&["enters", "tapped"])?;
                State::Unless
            }
            State::Unless => {
                cursor.enter("replacement.controlled-count-entry.controller");
                cursor.consume("unless")?;
                State::CountQuery
            }
            State::CountQuery => {
                cursor.enter("replacement.controlled-count-entry.count-query");
                let condition = recognize_controlled_permanent_count_query(cursor)?;
                cursor.require(
                    condition.exclude_source,
                    "`other` before the counted selector",
                )?;
                frame.threshold = Some(condition.threshold);
                frame.direction = Some(condition.direction);
                frame.predicate = Some(condition.predicate);
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("replacement.controlled-count-entry.terminator");
                cursor.consume(".")?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("replacement.controlled-count-entry.complete");
                cursor.finish()?;
                return Ok(ControlledCountEntryNode {
                    source: cursor.require_value(frame.source, "the entering source")?,
                    threshold: cursor.require_value(frame.threshold, "the count threshold")?,
                    direction: cursor.require_value(frame.direction, "the comparison direction")?,
                    predicate: cursor
                        .require_value(frame.predicate, "the counted permanent predicate")?,
                });
            }
        };
    }
}
