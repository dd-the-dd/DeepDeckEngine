//! Entry replacement `SOURCE enters with COUNT KIND counter(s) on it` (CR 614.1c).
//!
//! The counters are placed as the permanent enters, so the result is a
//! replacement over the source's own entry rather than a trigger. Subject,
//! counter kind and quantity are independent children: the subject is the
//! shared source reference, the kind is the shared counter-name child, and
//! the quantity is the shared numeric atom optionally scaled by the shared
//! `for each` count. A trailing `if CONDITION` (CR 614.1c with a condition
//! checked as the permanent enters) reuses the shared static condition.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, NumericAtom, NumericExpression, StaticConditionContext,
    StaticConditionNode, for_each_count_starts, recognize_counter_name, recognize_for_each_count,
    recognize_numeric_atom, recognize_source_reference, recognize_static_condition,
    scale_by_for_each_count,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEntryCountersNode {
    pub source: LocalObjectReference,
    pub counter: crate::oracle::counter_name::CounterName,
    pub count: NumericExpression,
    /// Checked once, as the source enters (CR 614.1c).
    pub condition: Option<StaticConditionNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Source,
    Event,
    Relation,
    Quantity,
    CounterKind,
    CounterNoun,
    Recipient,
    Scale,
    Condition,
    Terminator,
    Complete,
}

/// Recognizes the entry-counter replacement for the current source. A
/// quantity that is not a literal, an article or `X` (for example `a number
/// of ... equal to ...`) and any trailing condition fail at their own state.
pub(in crate::oracle::canonical) fn recognize_source_entry_counters(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SourceEntryCountersNode> {
    let mut state = State::Source;
    let mut source = None;
    let mut counter = None;
    let mut count = None;
    let mut condition = None;
    loop {
        state = match state {
            State::Source => {
                cursor.enter("replacement.entry-counters.source");
                source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                State::Event
            }
            State::Event => {
                cursor.enter("replacement.entry-counters.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("enters"),
                    "the `enters` event",
                )?;
                State::Relation
            }
            State::Relation => {
                cursor.enter("replacement.entry-counters.relation");
                cursor.consume("with")?;
                State::Quantity
            }
            State::Quantity => {
                cursor.enter("replacement.entry-counters.quantity");
                let atom = recognize_numeric_atom(cursor)?;
                cursor.require(
                    !matches!(atom, NumericAtom::Integer(value) if value <= 0),
                    "a positive counter quantity",
                )?;
                count = Some(NumericExpression::Atom(atom));
                State::CounterKind
            }
            State::CounterKind => {
                cursor.enter("replacement.entry-counters.kind");
                counter = Some(recognize_counter_name(cursor)?);
                State::CounterNoun
            }
            State::CounterNoun => {
                cursor.enter("replacement.entry-counters.noun");
                let singular = matches!(
                    count.as_ref(),
                    Some(NumericExpression::Atom(NumericAtom::Integer(1)))
                );
                cursor.consume(if singular { "counter" } else { "counters" })?;
                State::Recipient
            }
            State::Recipient => {
                cursor.enter("replacement.entry-counters.recipient");
                cursor.consume("on")?;
                let pronoun = cursor
                    .peek()
                    .filter(|raw| {
                        ["it", "him", "her"]
                            .iter()
                            .any(|candidate| raw.eq_ignore_ascii_case(candidate))
                    })
                    .map(str::to_string);
                let pronoun = cursor
                    .require_value(pronoun, "the entering source pronoun `it`, `him` or `her`")?;
                cursor.consume(&pronoun)?;
                State::Scale
            }
            State::Scale => {
                cursor.enter("replacement.entry-counters.scale");
                if for_each_count_starts(cursor) {
                    let base = cursor.require_value(count.take(), "a counter quantity")?;
                    let counted = recognize_for_each_count(cursor)?;
                    count = Some(cursor.require_value(
                        scale_by_for_each_count(&base, counted),
                        "a literal counter quantity multiplied by `for each`",
                    )?);
                }
                State::Condition
            }
            State::Condition => {
                cursor.enter("replacement.entry-counters.condition");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
                {
                    cursor.consume("if")?;
                    condition = Some(recognize_static_condition(
                        cursor,
                        StaticConditionContext {
                            source_subject: true,
                            name_links,
                            name_failure,
                        },
                    )?);
                }
                State::Terminator
            }
            State::Terminator => {
                cursor.enter("replacement.entry-counters.terminator");
                cursor.consume(".")?;
                State::Complete
            }
            State::Complete => {
                cursor.enter("replacement.entry-counters.complete");
                cursor.finish()?;
                return Ok(SourceEntryCountersNode {
                    source: cursor.require_value(source, "the entering source")?,
                    counter: cursor.require_value(counter, "a counter name")?,
                    count: cursor.require_value(count, "a counter quantity")?,
                    condition,
                });
            }
        };
    }
}
