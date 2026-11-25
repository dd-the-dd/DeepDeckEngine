use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, OracleTokenCursor};
use super::{LocalObjectReference, recognize_source_reference};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceTriggerEventKind {
    Entered,
    Died,
    Tapped,
    /// `transforms into FACE` (CR 701.28): the source turned to the named
    /// face; `transformed_into` keeps that face's name.
    Transformed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceTriggerEnvelopeNode {
    pub source: LocalObjectReference,
    pub event: SourceTriggerEventKind,
    /// Second transition of a `enters or dies` union, in written order.
    pub alternative: Option<SourceTriggerEventKind>,
    /// The face a `transforms into FACE` event names, as resolved by the
    /// ability's name links. The ability parent checks that it names the
    /// face that prints the ability.
    pub transformed_into: Option<String>,
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

/// Whether `transforms into FACE` is read: only the shared event clause,
/// whose ability parent checks the named face, reads it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TransformReading {
    Read,
    NotRead,
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
    match recognize_source_trigger_event_attempt(
        cursor,
        name_links,
        name_failure,
        TransformReading::NotRead,
    ) {
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
    let outcome = recognize_source_trigger_event_attempt(
        cursor,
        name_links,
        name_failure,
        TransformReading::Read,
    );
    if matches!(outcome, SourceTriggerEnvelopeAttempt::NotClaimed) {
        cursor.rollback(checkpoint);
    }
    outcome
}

/// `transforms into FACE`: the third-person `transform` lemma, `into` and
/// a name the ability's links resolve to a face of this card.
fn transformed_into_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        super::is_verb_form(
            raw,
            super::VerbLemma::Transform,
            super::VerbForm::ThirdPersonSingular,
        )
    }) && cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("into"))
}

fn recognize_transformed_into(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
) -> Result<String, MachineFailure> {
    cursor.enter("trigger.source-program.transformed-into");
    let verb = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&verb)?;
    cursor.consume("into")?;
    cursor.enter("trigger.source-program.transformed-face");
    let position = cursor.position();
    let link = name_links
        .iter()
        .find(|link| {
            link.token_start == position
                && matches!(
                    link.kind,
                    crate::oracle::syntax::NameLinkKind::CurrentCard
                        | crate::oracle::syntax::NameLinkKind::CurrentFace
                )
        })
        .ok_or_else(|| cursor.fail("the name of a face of this card"))?;
    cursor.advance_to(link.token_end)?;
    Ok(link.canonical_name.clone())
}

fn recognize_source_trigger_event_attempt(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    transform: TransformReading,
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
        Some(_) if transform == TransformReading::Read && transformed_into_starts(cursor) => {
            SourceTriggerEventKind::Transformed
        }
        _ => return SourceTriggerEnvelopeAttempt::NotClaimed,
    };
    let mut transformed_into = None;
    if event == SourceTriggerEventKind::Transformed {
        match recognize_transformed_into(cursor, name_links) {
            Ok(face) => transformed_into = Some(face),
            Err(failure) => return SourceTriggerEnvelopeAttempt::Unpromoted(failure),
        }
    } else {
        let event_word = match event {
            SourceTriggerEventKind::Entered => "enters",
            SourceTriggerEventKind::Died => "dies",
            _ => "becomes",
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
    // `transforms into FACE or dies`: the source's death is the other
    // transition of a transformed face.
    let (other, other_word) = match event {
        SourceTriggerEventKind::Died => (SourceTriggerEventKind::Entered, "enters"),
        _ => (SourceTriggerEventKind::Died, "dies"),
    };
    // Only the other battlefield transition of the source forms this union;
    // an `or` before another verb or subject belongs to the shared event
    // clause (a source verb union or an occurrence union), never to this
    // envelope.
    if event != SourceTriggerEventKind::Tapped
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(other_word))
    {
        cursor.enter("trigger.source-program.event-union");
        if let Err(failure) = cursor.consume("or") {
            return SourceTriggerEnvelopeAttempt::Unpromoted(failure);
        }
        if let Err(failure) = cursor.consume_type("eventWord").and_then(|token| {
            cursor.require(
                token.raw.eq_ignore_ascii_case(other_word),
                "the other transition of the source (enters or dies)",
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
        transformed_into,
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
