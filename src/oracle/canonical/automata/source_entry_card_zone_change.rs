use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, TargetCardZoneChangeEffectNode, recognize_source_entered_event,
    recognize_target_card_zone_change_effect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEntryTargetCardZoneChangeNode {
    pub source: LocalObjectReference,
    pub effect: TargetCardZoneChangeEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceEntryCardZoneChangeState {
    Trigger,
    Event,
    Separator,
    Effect,
    Terminator,
    Complete,
}

/// Composes the shared source-entry event with the independent targeted-card
/// zone-change child. The effect is deliberately not source-linked: linkage
/// is a separate semantic relation and cannot be inferred from `exile`.
pub(in crate::oracle::canonical) fn recognize_source_entry_target_card_zone_change(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<SourceEntryTargetCardZoneChangeNode> {
    let mut state = SourceEntryCardZoneChangeState::Trigger;
    let mut source = None;
    let mut effect = None;
    loop {
        state = match state {
            SourceEntryCardZoneChangeState::Trigger => {
                cursor.enter("trigger.source-entry-card-zone-change.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("when"),
                    "the `when` trigger",
                )?;
                SourceEntryCardZoneChangeState::Event
            }
            SourceEntryCardZoneChangeState::Event => {
                cursor.enter("trigger.source-entry-card-zone-change.event");
                let event = recognize_source_entered_event(cursor, name_links, name_failure)?;
                let super::LocalTriggerEventNode::SourceEntered {
                    source: entered_source,
                } = event
                else {
                    unreachable!("source-entry child returns its typed event")
                };
                source = Some(entered_source);
                SourceEntryCardZoneChangeState::Separator
            }
            SourceEntryCardZoneChangeState::Separator => {
                cursor.enter("trigger.source-entry-card-zone-change.separator");
                cursor.consume(",")?;
                SourceEntryCardZoneChangeState::Effect
            }
            SourceEntryCardZoneChangeState::Effect => {
                cursor.enter("trigger.source-entry-card-zone-change.effect");
                effect = Some(recognize_target_card_zone_change_effect(cursor)?);
                SourceEntryCardZoneChangeState::Terminator
            }
            SourceEntryCardZoneChangeState::Terminator => {
                cursor.enter("trigger.source-entry-card-zone-change.terminator");
                cursor.consume(".")?;
                SourceEntryCardZoneChangeState::Complete
            }
            SourceEntryCardZoneChangeState::Complete => {
                cursor.enter("trigger.source-entry-card-zone-change.complete");
                cursor.finish()?;
                return Ok(SourceEntryTargetCardZoneChangeNode {
                    source: cursor.require_value(source, "an entering source")?,
                    effect: cursor.require_value(effect, "a targeted card-zone change")?,
                });
            }
        };
    }
}
