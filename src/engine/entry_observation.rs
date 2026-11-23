//! Immutable entry-event observation and non-target object references.
//! Uses the existing battlefield incarnation marker; never mints an identity,
//! declares a target, or recaptures an object at ability resolution.
use super::object_lki::BattlefieldTargetIdentity;
use super::*;

const OBSERVED_OBJECT_DECISION: &str = "__deepdeckObservedEventObjectV1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum ObservedEventKind {
    PermanentEntered,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObservedEventObject {
    event: ObservedEventKind,
    identity: BattlefieldTargetIdentity,
}

/// Captured once from the prepared entering snapshot BEFORE Saga initialization
/// or trigger publication. A later untap/control change cannot rewrite entry.
pub(super) struct EntryObservation {
    tapped: bool,
    object: Option<ObservedEventObject>,
}

impl EntryObservation {
    pub(super) fn capture(source: &CardInstance) -> Self {
        Self {
            tapped: source.tapped,
            object: GameEngine::tracked_zone_identity(source)
                .filter(|identity| !identity.is_empty())
                .map(|identity| ObservedEventObject {
                    event: ObservedEventKind::PermanentEntered,
                    identity: BattlefieldTargetIdentity {
                        instance_id: source.instance_id.clone(),
                        zone_identity: identity.to_string(),
                    },
                }),
        }
    }

    pub(super) fn matches_state(&self, event: &Value) -> bool {
        entry_state_supported(event)
            && event
                .get("entryState")
                .is_none_or(|state| state["tapped"].as_bool() == Some(self.tapped))
    }

    pub(super) fn add_object_decision(&self, decisions: &mut BTreeMap<String, Value>) {
        if let Some(object) = &self.object {
            decisions.insert(
                OBSERVED_OBJECT_DECISION.to_string(),
                json!({
                    "event":"permanentEntered",
                    "identity":{
                        "instanceId":object.identity.instance_id,
                        "zoneIdentity":object.identity.zone_identity,
                    },
                }),
            );
        }
        // An unprepared legacy entry supplies no observation. A consumer then
        // reports a missing capture; it never guesses/mints a late incarnation.
    }
}

pub(super) fn entry_state_supported(event: &Value) -> bool {
    event.get("entryState").is_none_or(|state| {
        matches!(
            value_kind(event),
            Some("enterBattlefield" | "permanentEntered")
        ) && state
            .as_object()
            .is_some_and(|fields| fields.len() == 1 && state["tapped"].is_boolean())
    })
}

pub(super) fn observed_object_reference_supported(reference: &Value) -> bool {
    reference.as_object().is_some_and(|fields| {
        fields.len() == 2
            && value_kind(reference) == Some("observedEventObject")
            && reference["event"].as_str() == Some("permanentEntered")
    })
}

/// Reference-producer validation, not an execution/preflight visitor. Nested
/// ability definitions establish their OWN event scope; a parent's observation
/// cannot leak into a granted spell/activated/triggered ability.
pub(super) fn reference_scope_supported(rule: &Value) -> bool {
    let mut pending = vec![(rule, false)];
    while let Some((value, inherited_scope)) = pending.pop() {
        match value {
            Value::Object(fields) => {
                if value_kind(value) == Some("observedEventObject") {
                    if !inherited_scope || !observed_object_reference_supported(value) {
                        return false;
                    }
                    continue;
                }
                let scope = match value_kind(value) {
                    Some("triggeredAbility") => {
                        let events = triggered_event_variants(&value["event"]);
                        !events.is_empty()
                            && events.iter().all(|event| {
                                matches!(
                                    value_kind(event),
                                    Some("enterBattlefield" | "permanentEntered")
                                )
                            })
                    }
                    Some(
                        "spellAbility"
                        | "prepareSpell"
                        | "activatedAbility"
                        | "manaAbility"
                        | "replacementEffect"
                        | "staticAbility"
                        | "dungeonRoom"
                        | "keywordAbility"
                        | "keywordAbilityGroup",
                    ) => false,
                    _ => inherited_scope,
                };
                pending.extend(fields.values().map(|child| (child, scope)));
            }
            Value::Array(children) => {
                pending.extend(children.iter().map(|child| (child, inherited_scope)));
            }
            _ => {}
        }
    }
    true
}

pub(super) fn node_requires_checkpoint(node: &Value) -> bool {
    matches!(
        value_kind(node),
        Some("untapPermanent" | "destroyPermanent")
    ) && value_kind(&node["permanent"]) == Some("observedEventObject")
}

impl GameEngine {
    pub(super) fn resolve_observed_event_permanent(
        &self,
        reference: &Value,
        source: &StackObject,
    ) -> Result<Option<String>, EngineError> {
        if !observed_object_reference_supported(reference) {
            return Err(EngineError::new(
                "unsupported observed event-object reference",
            ));
        }
        let captured = source
            .decisions
            .get(OBSERVED_OBJECT_DECISION)
            .ok_or_else(|| EngineError::new("missing observed event-object capture"))?;
        let object: ObservedEventObject = serde_json::from_value(captured.clone())
            .map_err(|_| EngineError::new("malformed observed event-object capture"))?;
        if object.identity.instance_id.is_empty() || object.identity.zone_identity.is_empty() {
            return Err(EngineError::new("empty observed event-object identity"));
        }
        let Some((player, index)) = self.permanent_position(&object.identity.instance_id) else {
            return Ok(None);
        };
        let current = &self.state.players[player].battlefield[index];
        let current_identity = object_identity::object_identity_of(current)?;
        if current.flags.get("phasedOut").copied().unwrap_or(false)
            || current_identity.zone_identity != object.identity.zone_identity
        {
            return Ok(None);
        }
        Ok(Some(object.identity.instance_id))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "entry_observation_tests.rs"]
mod tests;
