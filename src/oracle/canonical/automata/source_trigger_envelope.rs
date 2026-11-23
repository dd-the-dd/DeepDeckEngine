use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, OracleTokenCursor};
use super::{LocalObjectReference, recognize_source_reference};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceTriggerEventKind {
    Entered,
    Died,
    Tapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceTriggerEnvelopeNode {
    pub source: LocalObjectReference,
    pub event: SourceTriggerEventKind,
    /// Second transition of a `enters or dies` union, in written order.
    pub alternative: Option<SourceTriggerEventKind>,
    pub body_start: usize,
}

impl SourceTriggerEnvelopeNode {
    /// Transitions in written order; a union has two distinct entries.
    pub(in crate::oracle::canonical) fn events(&self) -> Vec<SourceTriggerEventKind> {
        std::iter::once(self.event)
            .chain(self.alternative)
            .collect()
    }

    pub(in crate::oracle::canonical) fn is_union(&self) -> bool {
        self.alternative.is_some()
    }
}

pub(in crate::oracle::canonical) enum SourceTriggerEnvelopeAttempt {
    NotClaimed,
    Unpromoted(MachineFailure),
    Recognized(SourceTriggerEnvelopeNode),
}

/// Reads a source-identity event and leaves every body token to EffectSequence.
/// The recognized event is the commit boundary; neither a cursor threshold nor
/// a later action word determines whether the envelope belongs to this family.
pub(in crate::oracle::canonical) fn try_recognize_source_trigger_envelope(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> SourceTriggerEnvelopeAttempt {
    let checkpoint = cursor.checkpoint();
    let outcome = recognize_source_trigger_envelope_attempt(cursor, name_links, name_failure);
    if matches!(outcome, SourceTriggerEnvelopeAttempt::NotClaimed) {
        cursor.rollback(checkpoint);
    }
    outcome
}

fn recognize_source_trigger_envelope_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> SourceTriggerEnvelopeAttempt {
    cursor.enter("trigger.source-program.trigger");
    let Some(trigger) = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("when") || raw.eq_ignore_ascii_case("whenever"))
    else {
        return SourceTriggerEnvelopeAttempt::NotClaimed;
    };
    let trigger = trigger.to_string();
    if cursor.consume(&trigger).is_err() {
        return SourceTriggerEnvelopeAttempt::NotClaimed;
    }
    match recognize_source_trigger_event_attempt(cursor, name_links, name_failure) {
        SourceTriggerEnvelopeAttempt::Recognized(mut node) => {
            cursor.enter("trigger.source-program.separator");
            if let Err(failure) = cursor.consume(",") {
                return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
            }
            node.body_start = cursor.position();
            SourceTriggerEnvelopeAttempt::Recognized(node)
        }
        outcome => outcome,
    }
}

/// The source transition after the trigger word, for the shared event
/// clause (`event_clause.rs`), which owns the trigger word and separator.
/// `body_start` is the position after the event; the clause updates it.
pub(in crate::oracle::canonical) fn try_recognize_source_trigger_event(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> SourceTriggerEnvelopeAttempt {
    let checkpoint = cursor.checkpoint();
    let outcome = recognize_source_trigger_event_attempt(cursor, name_links, name_failure);
    if matches!(outcome, SourceTriggerEnvelopeAttempt::NotClaimed) {
        cursor.rollback(checkpoint);
    }
    outcome
}

fn recognize_source_trigger_event_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> SourceTriggerEnvelopeAttempt {
    cursor.enter("trigger.source-program.source");
    let source = match recognize_source_reference(cursor, name_links, name_failure) {
        Ok(source @ LocalObjectReference::SourceSelf) => source,
        _ => return SourceTriggerEnvelopeAttempt::NotClaimed,
    };
    cursor.enter("trigger.source-program.event");
    let event = match cursor.peek() {
        Some(raw) if raw.eq_ignore_ascii_case("enters") => SourceTriggerEventKind::Entered,
        Some(raw) if raw.eq_ignore_ascii_case("dies") => SourceTriggerEventKind::Died,
        Some(raw) if raw.eq_ignore_ascii_case("becomes") => SourceTriggerEventKind::Tapped,
        _ => return SourceTriggerEnvelopeAttempt::NotClaimed,
    };
    let event_word = match event {
        SourceTriggerEventKind::Entered => "enters",
        SourceTriggerEventKind::Died => "dies",
        SourceTriggerEventKind::Tapped => "becomes",
    };
    let event_token_type = if event == SourceTriggerEventKind::Tapped {
        "actionWord"
    } else {
        "eventWord"
    };
    if let Err(failure) = cursor.consume_type(event_token_type).and_then(|token| {
        cursor.require(
            token.raw.eq_ignore_ascii_case(event_word),
            "the recognized source event",
        )
    }) {
        return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
    }
    if event == SourceTriggerEventKind::Tapped {
        cursor.enter("trigger.source-program.tapped-transition");
        if let Err(failure) = cursor.consume("tapped") {
            return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
        }
    }
    if let Err(failure) = consume_optional_entered_zone(cursor, event) {
        return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
    }
    let mut alternative = None;
    if event != SourceTriggerEventKind::Tapped
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
    {
        // Only the proved battlefield transitions form a union; any other
        // second verb stays an explicit failure at this state.
        cursor.enter("trigger.source-program.event-union");
        if let Err(failure) = cursor.consume("or") {
            return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
        }
        let (other, other_word) = match event {
            SourceTriggerEventKind::Entered => (SourceTriggerEventKind::Died, "dies"),
            _ => (SourceTriggerEventKind::Entered, "enters"),
        };
        if let Err(failure) = cursor.consume_type("eventWord").and_then(|token| {
            cursor.require(
                token.raw.eq_ignore_ascii_case(other_word),
                "the other battlefield transition of the source (enters or dies)",
            )
        }) {
            return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
        }
        if let Err(failure) = consume_optional_entered_zone(cursor, other) {
            return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
        }
        alternative = Some(other);
    }
    SourceTriggerEnvelopeAttempt::Recognized(SourceTriggerEnvelopeNode {
        source,
        event,
        alternative,
        body_start: cursor.position(),
    })
}

/// The older Oracle form names the same battlefield transition explicitly.
fn consume_optional_entered_zone(
    cursor: &mut OracleTokenCursor<'_>,
    event: SourceTriggerEventKind,
) -> Result<(), MachineFailure> {
    if event == SourceTriggerEventKind::Entered
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
    {
        cursor.enter("trigger.source-program.entered-zone");
        cursor.consume_phrase(&["the", "battlefield"])?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "source_trigger_envelope_tests.rs"]
mod tests;
