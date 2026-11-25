//! The object an effect names as the source of its damage (CR 120.1).
//!
//! A damage action may name its source by any typed object reference: the
//! ability's own source, a declared target, the permanent the source is
//! attached to, the object of the triggering event, or an object an earlier
//! action of the same program bound. Each kind keeps its own rule when the
//! object is gone:
//!
//! - a declared or attached source that is no longer legal deals no damage
//!   (CR 608.2b);
//! - the event's object deals the damage as it last existed on the
//!   battlefield (CR 608.2h, 609.7): its authenticated departure capture
//!   supplies the effective characteristics, never a guessed snapshot;
//! - an object bound earlier in the same resolution has no departure capture
//!   of its own, so its absence is an explicit error, never a substitution.
use super::*;

/// The decision key under which a death record keeps the battlefield
/// incarnation of the object that died, so a later read of that object as a
/// damage source can authenticate its last-known information.
pub(in super::super) const DEPARTED_EVENT_OBJECT_IDENTITY: &str =
    "triggeringPermanentDepartedIdentity";

const EVENT_OBJECT_ID: &str = "triggeringPermanentId";
const EVENT_OBJECT_IDENTITY: &str = "triggeringPermanentIdentity";

/// The value a death record stores for `DEPARTED_EVENT_OBJECT_IDENTITY`:
/// the incarnation of the dying object, or `null` when it has none.
pub(in super::super) fn departed_identity_value(died: &CardInstance) -> Value {
    GameEngine::tracked_zone_identity(died)
        .filter(|identity| !identity.is_empty())
        .map_or(Value::Null, |identity| Value::String(identity.to_string()))
}

/// The event-object references whose departed object still deals damage
/// with its last known information.
pub(in super::super) fn is_event_object_damage_source(reference: &Value) -> bool {
    matches!(
        value_kind(reference),
        Some("triggeringPermanent" | "observedEventObject")
    )
}

/// The damage source an effect resolved to: its object id and, when the
/// object is no longer where the engine can read it, the effective profile
/// it last had.
pub(in super::super) struct EffectDamageSource {
    pub instance_id: String,
    pub profile: Option<super::super::source_lki::ResolvedDamageSource>,
}

impl GameEngine {
    /// Resolves the `source` of a `dealDamage` effect. `Ok(None)` is a
    /// source that deals no damage (an illegal declared or attached object,
    /// CR 608.2b). An omitted or unrecognized reference keeps the historical
    /// ability-source reading; every typed object reference uses its own.
    pub(in super::super) fn resolve_effect_damage_source(
        &self,
        source: Option<&Value>,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<Option<EffectDamageSource>, EngineError> {
        let Some(reference) = source else {
            return Ok(Some(self.ability_damage_source(stack_object)));
        };
        if is_event_object_damage_source(reference) {
            return self
                .event_object_damage_source(reference, stack_object, bindings)
                .map(Some);
        }
        let resolved = self
            .resolve_target(reference, stack_object, bindings)
            .and_then(|target| match target {
                TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                    Some(instance_id)
                }
                TargetRef::Player { .. } | TargetRef::StackObject { .. } => None,
            });
        match value_kind(reference) {
            Some("self" | "abilitySource") => Ok(Some(self.ability_damage_source(stack_object))),
            // CR 608.2b: an illegal declared object, or an attached or
            // combat-related object that is no longer there, deals nothing.
            Some(
                "chosenTarget" | "chosenTargets" | "attachedPermanent" | "combatCounterpart"
                | "blockingPermanent",
            ) => Ok(resolved.map(|instance_id| EffectDamageSource {
                instance_id,
                profile: None,
            })),
            // An object this resolution bound (a created token, the card a
            // move put onto the battlefield, a captured delayed operand): it
            // deals damage while it is a permanent. A captured delayed
            // operand that became a new object binds nothing (CR 400.7) and
            // deals nothing; a bound object that left during this resolution
            // has no departure capture, which is an explicit error.
            Some("singleBoundObject" | "boundObject" | "boundObjects" | "boundValue") => {
                match resolved {
                    Some(instance_id) if self.permanent_position(&instance_id).is_some() => {
                        Ok(Some(EffectDamageSource {
                            instance_id,
                            profile: None,
                        }))
                    }
                    Some(_) => Err(EngineError::new(
                        "a bound damage source left the battlefield without last-known information",
                    )),
                    None => Ok(None),
                }
            }
            _ => Ok(Some(match resolved {
                Some(instance_id) => EffectDamageSource {
                    instance_id,
                    profile: None,
                },
                None => self.ability_damage_source(stack_object),
            })),
        }
    }

    /// The ability's own source. A resolving spell has left the stack; its
    /// characteristics remain those of the spell for replacement effects that
    /// examine the damage source (CR 120.3, 609.7).
    fn ability_damage_source(&self, stack_object: &StackObject) -> EffectDamageSource {
        EffectDamageSource {
            instance_id: stack_object.card.instance_id.clone(),
            profile: stack_object_is_spell(stack_object)
                .then(|| self.resolve_damage_source_lki(stack_object).ok())
                .flatten(),
        }
    }

    /// The triggering event's object as a damage source: the live object
    /// while it is the incarnation the event saw, else the object as it
    /// last existed on the battlefield (CR 608.2h). The departure capture is
    /// found by the incarnation the trigger recorded; without one the read
    /// fails explicitly instead of borrowing another object's traits.
    pub(in super::super) fn event_object_damage_source(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Result<EffectDamageSource, EngineError> {
        if let Some(TargetRef::Permanent { instance_id }) =
            self.resolve_target(reference, stack_object, bindings)
        {
            return Ok(EffectDamageSource {
                instance_id,
                profile: None,
            });
        }
        let instance_id = stack_object
            .decisions
            .get(EVENT_OBJECT_ID)
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::new("event-object damage source has no event record"))?;
        let identity = [EVENT_OBJECT_IDENTITY, DEPARTED_EVENT_OBJECT_IDENTITY]
            .iter()
            .find_map(|key| {
                stack_object
                    .decisions
                    .get(*key)
                    .and_then(Value::as_str)
                    .filter(|identity| !identity.is_empty())
            })
            .ok_or_else(|| {
                EngineError::new("departed event-object damage source has no captured incarnation")
            })?;
        let departure = self
            .departed_incarnation(instance_id, identity)
            .ok_or_else(|| {
                EngineError::new(
                    "departed event-object damage source has no last-known information",
                )
            })?;
        let profile = self.resolve_damage_source_lki(departure)?;
        if !profile.last_known {
            return Err(EngineError::new(
                "departed event-object damage source capture is not last-known information",
            ));
        }
        Ok(EffectDamageSource {
            instance_id: instance_id.to_string(),
            profile: Some(profile),
        })
    }
}
