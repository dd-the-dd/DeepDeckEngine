//! Designated object target identity, carried by private stack decisions.
//!
//! Physical instance ids can be reused after a zone change. Declaration is the
//! commitment boundary; resolution compares the already captured identity,
//! never the identity of whatever happens to occupy that id at resolution.

use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::target_numeric_lki::{
    BattlefieldTargetIdentity, NumericTargetBinding, PreparedTargetDeparture,
    TargetNumericReadError,
};
use super::{CastingDeclaration, GameEngine, StackObject, TargetRef};

const CAPTURED_TARGETS: &str = "__deepdeckTargetBattlefieldIdentitiesV1";
const CAPTURED_CARD_TARGETS: &str = "__deepdeckTargetCardIdentitiesV1";

/// The same Magic-zone address used by movement/admission, not a physical
/// player-vector address for shared zones.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CapturedCardTargetBinding {
    identity: super::object_identity::ObjectIdentity,
    zone: super::card_zone_transition::ObjectZone,
}

impl GameEngine {
    /// Locate one physical carrier without choosing the first duplicate or
    /// assigning an identity. BF/outside-game placement means it is no longer
    /// an off-BF Card target; stack objects have their own TargetRef domain.
    fn live_card_target_binding(
        &self,
        instance_id: &str,
    ) -> Result<Option<CapturedCardTargetBinding>, super::EngineError> {
        let Some(source) = self.prepare_card_movement_source(instance_id, None)? else {
            return Ok(None);
        };
        if !source.zone.is_card_target_zone() {
            return Ok(None);
        }
        Ok(Some(CapturedCardTargetBinding {
            identity: source.identity,
            zone: source.zone,
        }))
    }

    pub(super) fn captured_card_target_binding(
        stack: &StackObject,
        target_id: &str,
    ) -> Result<CapturedCardTargetBinding, super::EngineError> {
        let Some(TargetRef::Card { instance_id }) = stack.targets.get(target_id) else {
            return Err(super::EngineError::new(
                "Card target identity requires a Card designation",
            ));
        };
        let value = stack
            .decisions
            .get(CAPTURED_CARD_TARGETS)
            .and_then(|captures| captures.get(target_id))
            .ok_or_else(|| super::EngineError::new("missing captured Card target identity"))?;
        let binding: CapturedCardTargetBinding = serde_json::from_value(value.clone())
            .map_err(|_| super::EngineError::new("malformed captured Card target identity"))?;
        if instance_id.is_empty()
            || binding.identity.instance_id != *instance_id
            || binding.identity.zone_identity.is_empty()
            || !binding.zone.is_card_target_zone()
            || binding.zone.owner().is_some_and(str::is_empty)
        {
            return Err(super::EngineError::new(
                "captured Card target identity does not match its designation",
            ));
        }
        Ok(binding)
    }

    /// Authentication precedes resolution pop and is distinct from legality.
    /// A real departure is not missing proof; malformed/missing/duplicate
    /// context is unsupported rather than an approximation by target fizzle.
    pub(super) fn validate_card_target_authentication(
        &self,
        stack: &StackObject,
        target_id: &str,
    ) -> Result<(), super::EngineError> {
        let binding = Self::captured_card_target_binding(stack, target_id)?;
        if binding
            .zone
            .owner()
            .is_some_and(|owner| !self.state.players.iter().any(|p| p.id == owner))
        {
            return Err(super::EngineError::new(
                "captured Card target has an unknown zone owner",
            ));
        }
        self.live_card_target_binding(&binding.identity.instance_id)?;
        Ok(())
    }

    pub(super) fn captured_target_numeric_binding(
        stack: &StackObject,
        target_id: &str,
    ) -> Result<NumericTargetBinding, TargetNumericReadError> {
        let TargetRef::Permanent { instance_id } = stack
            .targets
            .get(target_id)
            .ok_or(TargetNumericReadError::MissingDesignation)?
        else {
            return Err(TargetNumericReadError::UnsupportedTargetKind);
        };
        let captured = stack
            .decisions
            .get(CAPTURED_TARGETS)
            .and_then(|captures| captures.get(target_id))
            .ok_or(TargetNumericReadError::MissingDesignation)?;
        if captured["instanceId"].as_str() != Some(instance_id)
            || captured["present"].as_bool() != Some(true)
        {
            return Err(TargetNumericReadError::MalformedCapture);
        }
        let zone_identity = captured["zoneIdentity"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(TargetNumericReadError::MissingIncarnation)?;
        let last_known = captured
            .get("lastKnownNumeric")
            .map(|value| {
                serde_json::from_value(value.clone())
                    .map_err(|_| TargetNumericReadError::MalformedCapture)
            })
            .transpose()?;
        Ok(NumericTargetBinding {
            identity: BattlefieldTargetIdentity {
                instance_id: instance_id.clone(),
                zone_identity: zone_identity.to_string(),
            },
            last_known,
        })
    }

    pub(super) fn captured_target_numeric_bindings(
        stack: &StackObject,
    ) -> BTreeMap<String, NumericTargetBinding> {
        stack
            .targets
            .keys()
            .filter_map(|id| {
                Self::captured_target_numeric_binding(stack, id)
                    .ok()
                    .map(|binding| (id.clone(), binding))
            })
            .collect()
    }

    pub(super) fn capture_pending_target_numeric_departure(
        stack: &mut [StackObject],
        departure: &PreparedTargetDeparture,
    ) {
        for object in stack {
            let captures = Self::captured_target_numeric_bindings(object);
            for (id, binding) in captures {
                if binding.identity == departure.identity && binding.last_known.is_none() {
                    // This existing slot was authenticated above; retargeting
                    // removes the whole slot, including any prior numeric LKI.
                    object
                        .decisions
                        .get_mut(CAPTURED_TARGETS)
                        .expect("validated target map")
                        .get_mut(&id)
                        .expect("validated target slot")["lastKnownNumeric"] =
                        json!(departure.values);
                }
            }
        }
    }

    pub(super) fn capture_target_incarnation(
        &self,
        decisions: &mut BTreeMap<String, Value>,
        target_id: &str,
        target: &TargetRef,
    ) -> Result<(), super::EngineError> {
        // Authenticate before changing even the caller's local decisions.
        if let TargetRef::Card { instance_id } = target {
            let binding = self.live_card_target_binding(instance_id)?.ok_or_else(|| {
                super::EngineError::new("Card target designation requires a present supported card")
            })?;
            let mut next = decisions.clone();
            if let Some(captures) = next
                .get_mut(CAPTURED_TARGETS)
                .and_then(Value::as_object_mut)
            {
                captures.remove(target_id);
            }
            let captures = next
                .entry(CAPTURED_CARD_TARGETS.to_string())
                .or_insert_with(|| json!({}));
            let captures = captures
                .as_object_mut()
                .ok_or_else(|| super::EngineError::new("malformed Card target capture map"))?;
            captures.insert(target_id.to_string(), json!(binding));
            *decisions = next;
            return Ok(());
        }
        let captured = if let TargetRef::Permanent { instance_id } = target {
            let (player, index) = self.permanent_position(instance_id).ok_or_else(|| {
                super::EngineError::new("target designation requires a present permanent")
            })?;
            let identity = super::object_identity::object_identity_of(
                &self.state.players[player].battlefield[index],
            )?;
            Some(
                json!({"instanceId": identity.instance_id, "present": true, "zoneIdentity": identity.zone_identity}),
            )
        } else {
            None
        };
        let mut next = decisions.clone();
        if let Some(captures) = next.get_mut(CAPTURED_CARD_TARGETS) {
            captures
                .as_object_mut()
                .ok_or_else(|| super::EngineError::new("malformed Card target capture map"))?
                .remove(target_id);
        }
        let captures = next
            .entry(CAPTURED_TARGETS.to_string())
            .or_insert_with(|| json!({}));
        if !captures.is_object() {
            *captures = json!({});
        }
        let captures = captures.as_object_mut().expect("internal object capture");
        captures.remove(target_id);
        if let Some(captured) = captured {
            captures.insert(target_id.to_string(), captured);
        }
        *decisions = next;
        Ok(())
    }

    pub(super) fn capture_declared_target_incarnations(
        &self,
        declaration: &mut CastingDeclaration,
    ) -> Result<(), super::EngineError> {
        let mut decisions = declaration.decisions.clone();
        for target_id in &declaration.target_order {
            if let Some(target) = declaration.targets.get(target_id) {
                self.capture_target_incarnation(&mut decisions, target_id, target)?;
            }
        }
        declaration.decisions = decisions;
        Ok(())
    }

    pub(super) fn target_incarnation_is_current(
        &self,
        stack: &StackObject,
        target_id: &str,
        target: &TargetRef,
    ) -> bool {
        if let TargetRef::Card { instance_id } = target {
            let Ok(captured) = Self::captured_card_target_binding(stack, target_id) else {
                return false;
            };
            return self
                .live_card_target_binding(instance_id)
                .is_ok_and(|current| current.as_ref() == Some(&captured));
        }
        let TargetRef::Permanent { instance_id } = target else {
            return true;
        };
        let Some(captures) = stack.decisions.get(CAPTURED_TARGETS) else {
            // Old serialized stacks and handwritten implicit-target callers did
            // not capture an incarnation. Keep their existing behavior, but do
            // not claim those historical states are protected by this proof.
            return true;
        };
        let captured = &captures[target_id];
        if captured["instanceId"].as_str() != Some(instance_id)
            || captured["present"].as_bool() != Some(true)
        {
            return false;
        }
        self.permanent_position(instance_id)
            .is_some_and(|(player, index)| {
                let current = &self.state.players[player].battlefield[index];
                captured.get("zoneIdentity").is_some_and(|identity| {
                    identity.as_str() == Self::tracked_zone_identity(current)
                        && (identity.is_null() || identity.is_string())
                })
            })
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "target_incarnation_tests.rs"]
mod tests;
