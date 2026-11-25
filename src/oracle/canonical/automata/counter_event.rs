//! Counter placement events (CR 122.6): counters put on a permanent.
//!
//! ```text
//! CounterEvent := Batch Counters "are" "put" "on" Recipient
//!               | Actor ("put" | "puts") Batch Counters "on" Recipient
//!               | ("a" | "an") COUNTER_NAME "counter" "is" "put" "on" Recipient
//!               | "the" ORDINAL COUNTER_NAME "counter" "is" "put" "on" Recipient
//! Batch        := "one" "or" "more"
//! Counters     := [COUNTER_NAME] "counters"
//! Recipient    := SOURCE | ("a" | "an" | "another") PermanentQuery
//! ```
//!
//! The counter kind, the recipient, the placing player and the occurrence
//! are independent attributes. `one or more` observes one placement on one
//! permanent; the singular `a counter is put on` observes each counter of a
//! placement (CR 122.6, 603.2c); `the fourth plan counter is put on` observes
//! the placement that brings the count of that kind to four.
use crate::oracle::{
    counter_name::CounterName,
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EventActorScope, LocalObjectReference, ObjectKind, ObjectQueryNode, recognize_counter_name,
    recognize_object_query, recognize_source_reference, source_reference_starts_at,
    try_recognize_event_actor_scope,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterRecipient {
    Source(LocalObjectReference),
    Query {
        query: ObjectQueryNode,
        exclude_source: bool,
    },
}

/// Which counters of one placement are occurrences of the event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterPlacementOccurrence {
    /// `one or more COUNTERS are put on`: one occurrence per placement.
    Placement,
    /// `a COUNTER counter is put on`: one occurrence per counter put.
    EachCounter,
    /// `the ORDINAL COUNTER counter is put on`: the placement that puts the
    /// counter of that rank, the count of that kind crossing N.
    Ordinal(i64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterPlacementEventNode {
    /// The player who puts the counters, when the event names one.
    pub placed_by: Option<EventActorScope>,
    /// `None` for counters of any kind.
    pub counter: Option<CounterName>,
    pub recipient: CounterRecipient,
    pub occurrence: CounterPlacementOccurrence,
}

fn peek_is(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn recognize_counter_batch(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CounterName>> {
    cursor.enter("event.counters.batch");
    cursor.consume_phrase(&["one", "or", "more"])?;
    cursor.enter("event.counters.kind");
    let counter = if peek_is(cursor, 0, "counters") {
        None
    } else {
        Some(recognize_counter_name(cursor)?)
    };
    cursor.enter("event.counters.noun");
    cursor.consume("counters")?;
    Ok(counter)
}

/// `a COUNTER counter` / `the ORDINAL COUNTER counter`: one named counter of
/// the singular passive event.
fn recognize_singular_counter(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(CounterName, CounterPlacementOccurrence)> {
    cursor.enter("event.counters.singular");
    let occurrence = if peek_is(cursor, 0, "the") {
        cursor.consume("the")?;
        cursor.enter("event.counters.ordinal");
        let rank = super::recognize_ordinal_atom(cursor)?;
        cursor.require(rank >= 1, "a positive counter rank")?;
        CounterPlacementOccurrence::Ordinal(rank)
    } else {
        let article = cursor
            .peek()
            .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
            .map(str::to_string)
            .ok_or_else(|| cursor.fail("`a`, `an` or `the ORDINAL` before one counter"))?;
        cursor.consume(&article)?;
        CounterPlacementOccurrence::EachCounter
    };
    cursor.enter("event.counters.kind");
    let counter = recognize_counter_name(cursor)?;
    cursor.enter("event.counters.noun");
    cursor.consume("counter")?;
    cursor.enter("event.counters.passive");
    cursor.consume_phrase(&["is", "put", "on"])?;
    Ok((counter, occurrence))
}

/// `a|an|the` before a counter noun phrase whose noun is the singular
/// `counter`: the singular passive event (a lexical probe).
fn singular_counter_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let article = ["a", "an", "the"]
        .iter()
        .any(|word| peek_is(cursor, 0, word));
    article
        && (1..=4).any(|offset| {
            peek_is(cursor, offset, "counter")
                && peek_is(cursor, offset + 1, "is")
                && peek_is(cursor, offset + 2, "put")
        })
}

fn recognize_recipient(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<CounterRecipient> {
    cursor.enter("event.counters.recipient");
    if source_reference_starts_at(cursor.position(), cursor.peek(), name_links, name_failure) {
        return Ok(CounterRecipient::Source(recognize_source_reference(
            cursor,
            name_links,
            name_failure,
        )?));
    }
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| ["a", "an", "another"].contains(&raw.as_str()))
        .ok_or_else(|| cursor.fail("the source or `a`/`an`/`another` permanent"))?;
    cursor.consume(&head)?;
    cursor.enter("event.counters.recipient-query");
    let query = recognize_object_query(cursor)?;
    cursor.enter("event.counters.recipient-domain");
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Permanent)
            && query.predicate.has_nominal_head(),
        "a permanent query with a nominal head",
    )?;
    Ok(CounterRecipient::Query {
        query,
        exclude_source: head == "another",
    })
}

/// Recognizes a counter placement event after the trigger word. The
/// enclosing envelope owns the comma and the body.
pub(in crate::oracle::canonical) fn recognize_counter_placement_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<CounterPlacementEventNode> {
    cursor.enter("event.counters.subject");
    if singular_counter_starts(cursor) {
        let (counter, occurrence) = recognize_singular_counter(cursor)?;
        let recipient = recognize_recipient(cursor, name_links, name_failure)?;
        cursor.enter("event.counters.complete");
        return Ok(CounterPlacementEventNode {
            placed_by: None,
            counter: Some(counter),
            recipient,
            occurrence,
        });
    }
    let placed_by = try_recognize_event_actor_scope(cursor)?;
    let counter = if let Some(actor) = placed_by {
        cursor.enter("event.counters.action");
        cursor.consume(if actor == EventActorScope::Controller {
            "put"
        } else {
            "puts"
        })?;
        let counter = recognize_counter_batch(cursor)?;
        cursor.enter("event.counters.preposition");
        cursor.consume("on")?;
        counter
    } else {
        let counter = recognize_counter_batch(cursor)?;
        cursor.enter("event.counters.passive");
        cursor.consume_phrase(&["are", "put", "on"])?;
        counter
    };
    let recipient = recognize_recipient(cursor, name_links, name_failure)?;
    cursor.enter("event.counters.complete");
    Ok(CounterPlacementEventNode {
        placed_by,
        counter,
        recipient,
        occurrence: CounterPlacementOccurrence::Placement,
    })
}

#[cfg(test)]
#[path = "counter_event_tests.rs"]
mod tests;
