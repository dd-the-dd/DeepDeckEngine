use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, EffectSequenceNode, LocalObjectReference,
    recognize_effect_sequence_with_context, recognize_source_reference, source_reference_starts_at,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEntryEffectSequenceNode {
    pub source: LocalObjectReference,
    pub effects: EffectSequenceNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceEntryState {
    Trigger,
    Source,
    EntryEvent,
    Separator,
    Effects,
    Complete,
}

#[derive(Default)]
struct SourceEntryFrame {
    source: Option<LocalObjectReference>,
    effects: Option<EffectSequenceNode>,
}

/// Recognizes only the event envelope around an ordered effect sequence.
/// Atomic effects, source anaphora, numeric provenance, punctuation, and
/// connectors remain owned by the shared `EffectSequence` children.
pub(in crate::oracle::canonical) fn recognize_source_cast_x_entry_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SourceEntryEffectSequenceNode> {
    let mut state = SourceEntryState::Trigger;
    let mut frame = SourceEntryFrame::default();
    loop {
        state = match state {
            SourceEntryState::Trigger => {
                cursor.enter("trigger.source-entry.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("when"),
                    "the `when` trigger",
                )?;
                SourceEntryState::Source
            }
            SourceEntryState::Source => {
                cursor.enter("trigger.source-entry.source");
                frame.source = Some(recognize_source_reference(
                    cursor,
                    name_links,
                    name_failure,
                )?);
                SourceEntryState::EntryEvent
            }
            SourceEntryState::EntryEvent => {
                cursor.enter("trigger.source-entry.event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("enters"),
                    "the `enters` event",
                )?;
                SourceEntryState::Separator
            }
            SourceEntryState::Separator => {
                cursor.enter("trigger.source-entry.separator");
                cursor.consume(",")?;
                SourceEntryState::Effects
            }
            SourceEntryState::Effects => {
                cursor.enter("trigger.source-entry.effects");
                let source = frame
                    .source
                    .clone()
                    .ok_or_else(|| cursor.fail("the entering source binding"))?;
                let context = EffectRecognitionContext::for_source_cast_x(source)
                    .with_source_event_subject(source);
                frame.effects = Some(recognize_effect_sequence_with_context(
                    cursor,
                    name_links,
                    name_failure,
                    &context,
                )?);
                SourceEntryState::Complete
            }
            SourceEntryState::Complete => {
                cursor.enter("trigger.source-entry.complete");
                cursor.finish()?;
                return Ok(SourceEntryEffectSequenceNode {
                    source: cursor.require_value(frame.source, "the entering source")?,
                    effects: cursor.require_value(frame.effects, "an entry effect sequence")?,
                });
            }
        };
    }
}

pub(in crate::oracle::canonical) fn source_cast_x_entry_family_starts(
    tokens: &[crate::oracle::model::RecognizedEntity],
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> bool {
    tokens.first().is_some_and(|token| {
        token.token_type == "triggerWord" && token.raw.eq_ignore_ascii_case("when")
    }) && source_reference_starts_at(
        1,
        tokens.get(1).map(|token| token.raw.as_str()),
        name_links,
        name_failure,
    )
}
