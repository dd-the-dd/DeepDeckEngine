//! Effective damage traits of an identity-bound ability source.
//! This is owned by Source-LKI, independent of target-property read contexts.
use super::super::*;

const DAMAGE_CAPTURE: &str = "__deepdeckSourceDamageLkiV1";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in super::super) struct SourceDamageProperties {
    pub controller_id: String,
    pub owner_id: String,
    pub is_creature: bool,
    pub deathtouch: bool,
    pub lifelink: bool,
    pub wither: bool,
    pub infect: bool,
    pub colors: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SourceDamageCapture {
    version: u8,
    instance_id: String,
    zone_identity: String,
    properties: SourceDamageProperties,
}
pub(in super::super) struct ResolvedDamageSource {
    pub card: CardInstance,
    pub properties: SourceDamageProperties,
    pub last_known: bool,
}

/// Authenticate the existing departure capture without evaluating damage traits.
/// Source-relative pure properties borrow this proof, never a guessed snapshot.
fn authenticated_departure_capture(
    stack: &StackObject,
) -> Result<SourceDamageCapture, EngineError> {
    let captured = stack
        .decisions
        .get(DAMAGE_CAPTURE)
        .ok_or_else(|| EngineError::new("effective damage-source LKI was not captured"))?;
    let capture: SourceDamageCapture = serde_json::from_value(captured.clone())
        .map_err(|_| EngineError::new("malformed effective damage-source LKI"))?;
    if capture.version != 1
        || capture.instance_id != stack.card.instance_id
        || Some(capture.zone_identity.as_str()) != GameEngine::tracked_zone_identity(&stack.card)
    {
        return Err(EngineError::new("damage-source LKI identity mismatch"));
    }
    Ok(capture)
}

pub(super) fn validate_departed_source_identity(stack: &StackObject) -> Result<(), EngineError> {
    authenticated_departure_capture(stack).map(|_| ())
}

/// Reuse the existing incarnation/phasing admission without damage evaluation.
pub(super) fn authenticated_live_source<'a>(
    engine: &'a GameEngine,
    stack: &StackObject,
) -> Result<&'a CardInstance, EngineError> {
    let identity = GameEngine::tracked_zone_identity(&stack.card)
        .ok_or_else(|| EngineError::new("damage ability source has no captured incarnation"))?;
    let live = engine
        .state
        .players
        .iter()
        .flat_map(|p| p.battlefield.iter())
        .find(|card| {
            card.instance_id == stack.card.instance_id
                && GameEngine::tracked_zone_identity(card) == Some(identity)
        })
        .filter(|card| !card.flags.get("phasedOut").copied().unwrap_or(false))
        .ok_or_else(|| {
            EngineError::new(
                "damage source is absent or a different incarnation without effective LKI",
            )
        })?;
    Ok(live)
}

impl GameEngine {
    pub(in super::super) fn source_damage_properties(
        &self,
        source: &CardInstance,
    ) -> SourceDamageProperties {
        SourceDamageProperties {
            controller_id: source.controller.clone(),
            owner_id: source.owner.clone(),
            is_creature: self.permanent_is_creature(source),
            deathtouch: self.permanent_has_keyword(source, "deathtouch"),
            lifelink: self.permanent_has_keyword(source, "lifelink"),
            wither: self.permanent_has_keyword(source, "wither"),
            infect: self.permanent_has_keyword(source, "infect"),
            colors: self.permanent_color_symbols(source),
        }
    }
    pub(super) fn source_damage_departure_capture(
        &self,
        source: &CardInstance,
    ) -> Option<SourceDamageCapture> {
        if source.flags.get("phasedOut").copied().unwrap_or(false) {
            return None;
        }
        let identity =
            Self::tracked_zone_identity(source).filter(|identity| !identity.is_empty())?;
        Some(SourceDamageCapture {
            version: 1,
            instance_id: source.instance_id.clone(),
            zone_identity: identity.into(),
            properties: self.source_damage_properties(source),
        })
    }
    /// Strict self/ability-source resolver. No same-ID returned permanent or
    /// fresh stack snapshot can stand in for uncaptured historical traits.
    pub(in super::super) fn resolve_damage_source_lki(
        &self,
        stack: &StackObject,
    ) -> Result<ResolvedDamageSource, EngineError> {
        if super::source_has_departed(stack) {
            let capture = authenticated_departure_capture(stack)?;
            self.player_index(&capture.properties.controller_id)?;
            return Ok(ResolvedDamageSource {
                card: stack.card.clone(),
                properties: capture.properties,
                last_known: true,
            });
        }
        if stack_object_is_spell(stack) {
            // This object is the resolving spell after it was popped. Treating
            // it as a battlefield permanent would apply the wrong grant domain.
            let mut card = stack.card.clone();
            card.controller = stack.controller.clone();
            let properties = SourceDamageProperties {
                controller_id: stack.controller.clone(),
                owner_id: card.owner.clone(),
                is_creature: is_creature(&card.definition),
                deathtouch: has_keyword(&card.definition, "deathtouch"),
                lifelink: has_keyword(&card.definition, "lifelink"),
                wither: has_keyword(&card.definition, "wither"),
                infect: has_keyword(&card.definition, "infect"),
                colors: card_color_symbols(&card.definition),
            };
            return Ok(ResolvedDamageSource {
                card,
                properties,
                last_known: false,
            });
        }
        let live = authenticated_live_source(self, stack)?;
        Ok(ResolvedDamageSource {
            card: live.clone(),
            properties: self.source_damage_properties(live),
            last_known: false,
        })
    }
}

pub(super) fn record_damage_departure_capture(
    stack: &mut StackObject,
    capture: Option<&SourceDamageCapture>,
) {
    let Some(capture) = capture else {
        return;
    };
    if stack.card.instance_id != capture.instance_id
        || GameEngine::tracked_zone_identity(&stack.card) != Some(capture.zone_identity.as_str())
    {
        return;
    }
    // Fixed strings, integers, booleans and sets serialize without a fallible
    // custom serializer. Failure still leaves a missing producer, never zero.
    if let Ok(value) = serde_json::to_value(capture) {
        stack.decisions.insert(DAMAGE_CAPTURE.into(), value);
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "source_lki_damage_tests.rs"]
mod tests;
