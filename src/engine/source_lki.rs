//! Last-known information for abilities whose source leaves the battlefield.
//!
//! The four central exit paths call `capture_pending_source_lki`. Legacy
//! instruction branches that still remove a battlefield card directly (for
//! example Proteus Staff and old card-instruction shuffles) remain migration
//! debt and must be routed through those exits before claiming LKI support.

use super::{CardInstance, GameEngine, StackObject};

const SOURCE_LEFT_BATTLEFIELD: &str = "sourceLkiDeparted";
const SOURCE_LAST_KNOWN_POWER: &str = "sourceLkiPower";
const SOURCE_LAST_KNOWN_TOUGHNESS: &str = "sourceLkiToughness";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceNumericProperty {
    Power,
    Toughness,
}

pub(super) enum SourcePropertyInput<'a> {
    Live(&'a CardInstance),
    Captured(i32),
    Snapshot(&'a CardInstance),
}

impl GameEngine {
    /// Captures the effective characteristics of a permanent immediately before
    /// it changes zones. Pending abilities keep the old object's identity even
    /// when this engine later reuses the physical card's instance id for a new
    /// battlefield object after blink.
    pub(super) fn capture_pending_source_lki(&mut self, instance_id: &str) {
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return;
        };
        let source = self.state.players[player_index].battlefield[card_index].clone();
        let power = self.current_power(&source);
        let toughness = self.current_toughness(&source);
        capture_departing_source(&mut self.state.stack, &source, power, toughness);
    }
}

fn capture_departing_source(
    stack: &mut [StackObject],
    source: &CardInstance,
    power: i32,
    toughness: i32,
) {
    for object in stack.iter_mut().filter(|object| {
        object.ability_kind.is_some() && object.card.instance_id == source.instance_id
    }) {
        object.card = source.clone();
        object
            .decisions
            .insert(SOURCE_LEFT_BATTLEFIELD.to_string(), true.into());
        object
            .decisions
            .insert(SOURCE_LAST_KNOWN_POWER.to_string(), power.into());
        object
            .decisions
            .insert(SOURCE_LAST_KNOWN_TOUGHNESS.to_string(), toughness.into());
    }
}

/// Resolves which representation owns a source property. A departure marker
/// wins over a same-id live permanent so blink cannot turn an old ability into
/// an ability of the returned object. Unmarked absent sources retain the old
/// stack snapshot fallback used by abilities whose source was already gone
/// when the stack object was created.
pub(super) fn property_input<'a>(
    stack_object: &'a StackObject,
    live_source: Option<&'a CardInstance>,
    property: SourceNumericProperty,
) -> Option<SourcePropertyInput<'a>> {
    if stack_object
        .decisions
        .get(SOURCE_LEFT_BATTLEFIELD)
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        let key = match property {
            SourceNumericProperty::Power => SOURCE_LAST_KNOWN_POWER,
            SourceNumericProperty::Toughness => SOURCE_LAST_KNOWN_TOUGHNESS,
        };
        return stack_object
            .decisions
            .get(key)
            .and_then(|value| value.as_i64())
            .and_then(|value| i32::try_from(value).ok())
            .map(SourcePropertyInput::Captured);
    }
    Some(live_source.map_or_else(
        || SourcePropertyInput::Snapshot(&stack_object.card),
        SourcePropertyInput::Live,
    ))
}
