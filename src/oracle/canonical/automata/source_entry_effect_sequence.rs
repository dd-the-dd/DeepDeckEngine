use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectRecognitionContext, EffectSequenceNode, LocalObjectReference, LocalTriggerEventNode,
    recognize_effect_sequence_with_context, recognize_source_entered_event,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEntryEffectSequenceNode {
    pub source: LocalObjectReference,
    pub optional: bool,
    pub effects: EffectSequenceNode,
}

/// Generic parent for `When SOURCE enters, EFFECT_SEQUENCE`.
///
/// The event, every atomic action, connectors, bindings, and full consumption
/// stay owned by their existing shared child automata.
pub(in crate::oracle::canonical) fn recognize_source_entry_effect_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SourceEntryEffectSequenceNode> {
    cursor.enter("trigger.source-entry-sequence.trigger");
    cursor.consume("when")?;
    let LocalTriggerEventNode::SourceEntered { source } =
        recognize_source_entered_event(cursor, name_links, name_failure)?
    else {
        return Err(cursor.fail("the current source entering"));
    };
    cursor.enter("trigger.source-entry-sequence.separator");
    cursor.consume(",")?;
    cursor.enter("trigger.source-entry-sequence.optionality");
    let optional = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"));
    if optional {
        cursor.consume("you")?;
        cursor.consume("may")?;
    }
    let effects = recognize_effect_sequence_with_context(
        cursor,
        name_links,
        name_failure,
        &EffectRecognitionContext::for_source(source.clone()),
    )?;
    if optional {
        cursor.enter("trigger.source-entry-sequence.optional-scope");
        cursor.require(
            effects.effects.len() == 1 && effects.connectors.is_empty(),
            "exactly one atomic effect governed by 'you may'",
        )?;
    }
    cursor.enter("trigger.source-entry-sequence.complete");
    Ok(SourceEntryEffectSequenceNode {
        source,
        optional,
        effects,
    })
}

#[cfg(test)]
#[path = "source_entry_effect_sequence_tests.rs"]
mod tests;
