use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, EffectSequenceNode, LocalObjectReference,
    TriggerEventAlternativesNode, recognize_effect_sequence_with_context,
    recognize_entry_and_repeat_event_alternatives,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EntryOrRepeatTriggeredAbilityNode {
    pub events: TriggerEventAlternativesNode,
    pub effects: EffectSequenceNode,
}

/// Recognizes only the triggered-ability envelope. Event alternatives and the
/// ordered atomic effects remain owned by their shared child automata.
pub(in crate::oracle::canonical) fn recognize_entry_or_repeat_triggered_ability(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EntryOrRepeatTriggeredAbilityNode> {
    let events = recognize_entry_and_repeat_event_alternatives(cursor, name_links, name_failure)?;
    let context = EffectRecognitionContext {
        source: Some(LocalObjectReference::SourceSelf),
        source_cast_x: false,
        ..EffectRecognitionContext::default()
    };
    let effects =
        recognize_effect_sequence_with_context(cursor, name_links, name_failure, &context)?;
    cursor.enter("trigger.entry-or-repeat.complete");
    cursor.finish()?;
    Ok(EntryOrRepeatTriggeredAbilityNode { events, effects })
}
