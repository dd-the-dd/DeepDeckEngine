//! Last-known information for abilities whose source leaves the battlefield.
//!
//! The four central exit paths call `capture_pending_source_lki`. Legacy
//! instruction branches that still remove a battlefield card directly (for
//! example Proteus Staff and old card-instruction shuffles) remain migration
//! debt and must be routed through those exits before claiming LKI support.

use super::{CardInstance, GameEngine, StackObject};

#[path = "source_lki_damage.rs"]
mod damage;
pub(super) use damage::ResolvedDamageSource;

#[path = "source_lki_contexts.rs"]
mod contexts;
pub(super) use contexts::SourceLkiContexts;

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

pub(super) struct SourceObjectInput<'a> {
    pub(super) card: &'a CardInstance,
    pub(super) on_battlefield: bool,
    pub(super) last_known: bool,
}

pub(super) fn source_has_departed(stack_object: &StackObject) -> bool {
    stack_object
        .decisions
        .get(SOURCE_LEFT_BATTLEFIELD)
        .and_then(|value| value.as_bool())
        == Some(true)
}

/// Preserve the source snapshot when the physical card returns as a new
/// battlefield object. Missing historical zone markers are never invented.
pub(super) fn source_object_input<'a>(
    stack_object: &'a StackObject,
    live_source: Option<&'a CardInstance>,
) -> SourceObjectInput<'a> {
    let matching_live = live_source.filter(|live| {
        !source_has_departed(stack_object)
            && GameEngine::tracked_zone_identity(live)
                == GameEngine::tracked_zone_identity(&stack_object.card)
    });
    SourceObjectInput {
        card: matching_live.unwrap_or(&stack_object.card),
        on_battlefield: matching_live.is_some(),
        last_known: matching_live.is_none() && stack_object.ability_kind.is_some(),
    }
}

impl GameEngine {
    /// Source-relative pure reads require an admitted incarnation. A current
    /// same-ID object is insufficient after blink; only the existing departure
    /// capture can authorize last-known source data. No identity is minted here.
    pub(super) fn checked_battlefield_source_object<'a>(
        &'a self,
        stack: &'a StackObject,
    ) -> Result<SourceObjectInput<'a>, super::EngineError> {
        super::object_identity::object_identity_of(&stack.card)?;
        if super::stack_object_is_spell(stack) {
            return Err(super::EngineError::new(
                "source counter query requires a battlefield ability source",
            ));
        }
        if source_has_departed(stack) {
            damage::validate_departed_source_identity(stack)?;
            return Ok(SourceObjectInput {
                card: &stack.card,
                on_battlefield: false,
                last_known: true,
            });
        }
        // There is no authenticated phasing-LKI producer in this boundary.
        // Such a live source is explicitly unsupported, never read as present
        // or replaced by the activation snapshot. Other auth failures stay Err.
        let card = damage::authenticated_live_source(self, stack).map_err(|error| {
            super::EngineError::new(format!(
                "unsupported checked battlefield source context: {error}"
            ))
        })?;
        Ok(SourceObjectInput {
            card,
            on_battlefield: true,
            last_known: false,
        })
    }

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
        let damage = self.source_damage_departure_capture(&source);
        capture_departing_source(
            &mut self.state.stack,
            &source,
            power,
            toughness,
            damage.as_ref(),
        );
        // Immediate observers require a genuine matching incarnation. Legacy
        // stack snapshots retain their historical admission policy above.
        for object in self.source_lki_contexts.frames.iter_mut().filter(|object| {
            GameEngine::tracked_zone_identity(&source).is_some()
                && GameEngine::tracked_zone_identity(&source)
                    == GameEngine::tracked_zone_identity(&object.card)
        }) {
            capture_departing_source(
                std::slice::from_mut(object),
                &source,
                power,
                toughness,
                damage.as_ref(),
            );
        }
    }

    /// CR 602.2a/602.2h: an activated ability exists before its costs are
    /// paid. While its costs are committed, the pending ability subscribes to
    /// the same actual-departure capture as stack abilities, so a source
    /// sacrificed as a cost leaves authenticated last-known information on the
    /// ability that is then put on the stack. No snapshot is guessed here.
    pub(super) fn with_pending_activation_source<T>(
        &mut self,
        pending: StackObject,
        work: impl FnOnce(&mut Self) -> Result<T, super::EngineError>,
    ) -> Result<(T, StackObject), super::EngineError> {
        let depth = self.source_lki_contexts.frames.len();
        self.source_lki_contexts.frames.push(pending);
        let result = work(self);
        let frame = self.source_lki_contexts.frames.get(depth).cloned();
        self.source_lki_contexts.frames.truncate(depth);
        let value = result?;
        let frame = frame.ok_or_else(|| {
            super::EngineError::new("pending activated ability source frame is absent")
        })?;
        Ok((value, frame))
    }
}

fn capture_departing_source(
    stack: &mut [StackObject],
    source: &CardInstance,
    power: i32,
    toughness: i32,
    damage: Option<&damage::SourceDamageCapture>,
) {
    for object in stack.iter_mut().filter(|object| {
        object.ability_kind.is_some()
            && object.card.instance_id == source.instance_id
            && !source_has_departed(object)
    }) {
        damage::record_damage_departure_capture(object, damage);
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
    if source_has_departed(stack_object) {
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
