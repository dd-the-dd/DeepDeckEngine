use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ControlledCountDirection, LocalObjectReference, ObjectPredicate, OpponentCountConditionNode,
    recognize_controlled_permanent_count_query, recognize_opponent_count_query,
    recognize_source_reference,
};

/// The count condition that exempts the source from entering tapped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum EntryCountConditionNode {
    /// `you control N or more/fewer [other] PERMANENTS`.
    ControlledPermanents {
        threshold: i64,
        direction: ControlledCountDirection,
        predicate: ObjectPredicate,
    },
    /// `you have N or more/fewer opponents`.
    Opponents(OpponentCountConditionNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ControlledCountEntryNode {
    pub source: LocalObjectReference,
    pub condition: EntryCountConditionNode,
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
    condition: Option<EntryCountConditionNode>,
}

/// Recognizes an entering-state exemption based on a count condition of the
/// source's controller: how many matching permanents they control, or how
/// many opponents they have. Quantity, comparison direction and the counted
/// domain remain independent typed values from shared condition children.
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
                frame.condition = Some(
                    if cursor
                        .peek_ahead(1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("have"))
                    {
                        EntryCountConditionNode::Opponents(recognize_opponent_count_query(cursor)?)
                    } else {
                        // `other` is accepted but adds no restriction: the
                        // replacement is applied before the source exists on
                        // the battlefield (CR 614.12), so it cannot be counted.
                        let condition = recognize_controlled_permanent_count_query(cursor)?;
                        EntryCountConditionNode::ControlledPermanents {
                            threshold: condition.threshold,
                            direction: condition.direction,
                            predicate: condition.predicate,
                        }
                    },
                );
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
                    condition: cursor
                        .require_value(frame.condition, "the entering count condition")?,
                });
            }
        };
    }
}
