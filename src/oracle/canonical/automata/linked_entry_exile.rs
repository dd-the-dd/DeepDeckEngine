use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, ObjectKind, TargetListNode, TargetSelector, recognize_target_list,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LinkedExileSourceKindNode {
    Artifact,
    Aura,
    Battle,
    Creature,
    Enchantment,
    Equipment,
    Land,
    Permanent,
    Planeswalker,
    Room,
    Saga,
    Spacecraft,
    Vehicle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LinkedExileDurationNode {
    UntilSourceLeavesBattlefield {
        source: LocalObjectReference,
        source_kind: LinkedExileSourceKindNode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LinkedExileEffectNode {
    pub target: TargetListNode,
    pub duration: LinkedExileDurationNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SourceEntryLinkedExileNode {
    pub source: LocalObjectReference,
    pub source_kind: LinkedExileSourceKindNode,
    pub effect: LinkedExileEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkedEntryExileState {
    Trigger,
    Source,
    SourceKind,
    EntryEvent,
    Separator,
    Action,
    Target,
    DurationBoundary,
    DurationSource,
    DurationSourceKind,
    LeaveEvent,
    BattlefieldDeterminer,
    BattlefieldZone,
    Terminator,
    Complete,
}

#[derive(Default)]
struct LinkedEntryExileFrame {
    source_kind: Option<LinkedExileSourceKindNode>,
    target: Option<TargetListNode>,
    duration: Option<LinkedExileDurationNode>,
}

fn source_kind(raw: &str) -> Option<LinkedExileSourceKindNode> {
    match raw.to_ascii_lowercase().as_str() {
        "artifact" => Some(LinkedExileSourceKindNode::Artifact),
        "aura" => Some(LinkedExileSourceKindNode::Aura),
        "battle" => Some(LinkedExileSourceKindNode::Battle),
        "creature" => Some(LinkedExileSourceKindNode::Creature),
        "enchantment" => Some(LinkedExileSourceKindNode::Enchantment),
        "equipment" => Some(LinkedExileSourceKindNode::Equipment),
        "land" => Some(LinkedExileSourceKindNode::Land),
        "permanent" => Some(LinkedExileSourceKindNode::Permanent),
        "planeswalker" => Some(LinkedExileSourceKindNode::Planeswalker),
        "room" => Some(LinkedExileSourceKindNode::Room),
        "saga" => Some(LinkedExileSourceKindNode::Saga),
        "spacecraft" => Some(LinkedExileSourceKindNode::Spacecraft),
        "vehicle" => Some(LinkedExileSourceKindNode::Vehicle),
        _ => None,
    }
}

fn permanent_target_supported(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate.supports_domain(ObjectKind::Permanent)
        }
        TargetSelector::Any
        | TargetSelector::Union(_)
        | TargetSelector::Player(_)
        | TargetSelector::StackItem(_) => false,
    }
}

/// Recognizes a source-entry trigger whose atomic effect exiles one target
/// collection for exactly as long as that same source remains on the
/// battlefield. Target cardinality, predicate composition, controller scope,
/// and numeric comparisons remain owned by the shared target/selector child.
pub(in crate::oracle::canonical) fn recognize_source_entry_linked_exile(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SourceEntryLinkedExileNode> {
    let mut state = LinkedEntryExileState::Trigger;
    let mut frame = LinkedEntryExileFrame::default();

    loop {
        state = match state {
            LinkedEntryExileState::Trigger => {
                cursor.enter("trigger.linked-exile.trigger");
                let trigger = cursor.consume_type("triggerWord")?;
                cursor.require(
                    trigger.raw.eq_ignore_ascii_case("when"),
                    "the `when` trigger",
                )?;
                LinkedEntryExileState::Source
            }
            LinkedEntryExileState::Source => {
                cursor.enter("trigger.linked-exile.source");
                cursor.consume("this")?;
                LinkedEntryExileState::SourceKind
            }
            LinkedEntryExileState::SourceKind => {
                cursor.enter("trigger.linked-exile.source-kind");
                let token = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail("a permanent source kind"))?;
                cursor.advance_to(cursor.position() + 1)?;
                frame.source_kind = Some(source_kind(&token.raw).ok_or_else(|| {
                    cursor.fail_with_span(
                        "a permanent source kind",
                        token.index,
                        token.span_start,
                        token.span_end,
                        token.raw,
                    )
                })?);
                LinkedEntryExileState::EntryEvent
            }
            LinkedEntryExileState::EntryEvent => {
                cursor.enter("trigger.linked-exile.entry-event");
                let event = cursor.consume_type("eventWord")?;
                cursor.require(
                    event.raw.eq_ignore_ascii_case("enters"),
                    "the `enters` event",
                )?;
                LinkedEntryExileState::Separator
            }
            LinkedEntryExileState::Separator => {
                cursor.enter("trigger.linked-exile.separator");
                cursor.consume(",")?;
                LinkedEntryExileState::Action
            }
            LinkedEntryExileState::Action => {
                cursor.enter("effect.linked-exile.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    action.raw.eq_ignore_ascii_case("exile"),
                    "the `exile` action",
                )?;
                LinkedEntryExileState::Target
            }
            LinkedEntryExileState::Target => {
                cursor.enter("effect.linked-exile.target");
                let target = recognize_target_list(cursor)?;
                cursor.require(
                    target.slots.len() == 1,
                    "exactly one target slot for linked exile",
                )?;
                cursor.require(
                    target.slots.first().is_some_and(|slot| {
                        slot.cardinality.maximum > 0 && permanent_target_supported(&slot.selector)
                    }),
                    "a nonempty permanent target collection",
                )?;
                frame.target = Some(target);
                LinkedEntryExileState::DurationBoundary
            }
            LinkedEntryExileState::DurationBoundary => {
                cursor.enter("duration.boundary");
                cursor.consume("until")?;
                LinkedEntryExileState::DurationSource
            }
            LinkedEntryExileState::DurationSource => {
                cursor.enter("duration.source");
                cursor.consume("this")?;
                LinkedEntryExileState::DurationSourceKind
            }
            LinkedEntryExileState::DurationSourceKind => {
                cursor.enter("duration.source-kind");
                let token = cursor
                    .peek_token()
                    .cloned()
                    .ok_or_else(|| cursor.fail("the triggering source kind"))?;
                let duration_kind = source_kind(&token.raw).ok_or_else(|| {
                    cursor.fail_with_span(
                        "a permanent source kind",
                        token.index,
                        token.span_start,
                        token.span_end,
                        token.raw.clone(),
                    )
                })?;
                if Some(duration_kind) != frame.source_kind {
                    return Err(cursor.fail_with_span(
                        "the same source kind as the entry event",
                        token.index,
                        token.span_start,
                        token.span_end,
                        token.raw,
                    ));
                }
                cursor.advance_to(cursor.position() + 1)?;
                frame.duration = Some(LinkedExileDurationNode::UntilSourceLeavesBattlefield {
                    source: LocalObjectReference::SourceSelf,
                    source_kind: duration_kind,
                });
                LinkedEntryExileState::LeaveEvent
            }
            LinkedEntryExileState::LeaveEvent => {
                cursor.enter("duration.leave-event");
                cursor.consume("leaves")?;
                LinkedEntryExileState::BattlefieldDeterminer
            }
            LinkedEntryExileState::BattlefieldDeterminer => {
                cursor.enter("duration.battlefield-determiner");
                cursor.consume("the")?;
                LinkedEntryExileState::BattlefieldZone
            }
            LinkedEntryExileState::BattlefieldZone => {
                cursor.enter("duration.battlefield-zone");
                let zone = cursor.consume_type("zoneWord")?;
                cursor.require(
                    zone.raw.eq_ignore_ascii_case("battlefield"),
                    "the battlefield zone",
                )?;
                LinkedEntryExileState::Terminator
            }
            LinkedEntryExileState::Terminator => {
                cursor.enter("effect.linked-exile.terminator");
                cursor.consume(".")?;
                LinkedEntryExileState::Complete
            }
            LinkedEntryExileState::Complete => {
                cursor.enter("trigger.linked-exile.complete");
                cursor.finish()?;
                return Ok(SourceEntryLinkedExileNode {
                    source: LocalObjectReference::SourceSelf,
                    source_kind: cursor
                        .require_value(frame.source_kind, "a triggering source kind")?,
                    effect: LinkedExileEffectNode {
                        target: cursor
                            .require_value(frame.target, "a linked-exile target collection")?,
                        duration: cursor.require_value(
                            frame.duration,
                            "a duration linked to the triggering source",
                        )?,
                    },
                });
            }
        };
    }
}
