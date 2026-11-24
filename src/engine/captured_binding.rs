//! Bound objects a delayed trigger captured when it was created (CR 603.7c).
//! Each captured id travels with the identity of the object it named; the
//! trigger finds the object only while it is still that object (CR 400.7),
//! like the transported combat counterpart.
//!
//! A delayed range may also read a declared target, the source or the
//! creating event's object or player. Lowering replaces each such reference
//! by a `boundValue` and lists it under `captureReferences`; the trigger
//! records what each one names when it is created and, when it resolves,
//! binds only a player or an object that is still the same object.

use super::*;

/// The decision key holding the identities of a captured object binding.
pub(super) fn captured_identities_key(binding_id: &str) -> String {
    format!("{binding_id}#identities")
}

/// The decision key of the references a delayed trigger captured.
pub(super) const CAPTURED_REFERENCES_KEY: &str = "capturedReferences";

/// The reference kinds a delayed trigger may capture.
fn capturable_reference(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("chosenTarget" | "chosenObject") => reference["id"].is_string(),
        Some("self" | "triggeringPermanent" | "observedEventObject" | "triggeringPlayer") => true,
        _ => false,
    }
}

/// `captureReferences`: distinct bindings, each over one capturable
/// reference.
pub(super) fn captured_references_supported(captures: &Value) -> bool {
    captures.as_array().is_some_and(|captures| {
        !captures.is_empty()
            && captures.iter().all(|capture| {
                capture["binding"].is_string() && capturable_reference(&capture["reference"])
            })
            && captures
                .iter()
                .filter_map(|capture| capture["binding"].as_str())
                .collect::<BTreeSet<_>>()
                .len()
                == captures.len()
    })
}

impl GameEngine {
    /// The identities of the objects a binding names, captured with them.
    pub(super) fn captured_object_identities(&self, instance_ids: &[String]) -> Value {
        Value::Array(
            instance_ids
                .iter()
                .map(|instance_id| self.object_identity_value(instance_id))
                .collect(),
        )
    }

    /// The first object a delayed trigger captured under `binding_id`, while
    /// it is the same battlefield object it was at capture. `None` outside a
    /// captured-binding context (no identities recorded) or once the object
    /// left or became a new object.
    pub(super) fn captured_bound_object(
        &self,
        stack_object: &StackObject,
        binding_id: &str,
    ) -> Option<String> {
        let instance_id = stack_object
            .decisions
            .get(binding_id)?
            .as_array()?
            .first()?
            .as_str()?;
        let captured = stack_object
            .decisions
            .get(&captured_identities_key(binding_id))?
            .as_array()?
            .first()?;
        // An object captured without an identity cannot be told apart from a
        // later incarnation: it is never found.
        (!captured.is_null()
            && self.permanent_position(instance_id).is_some()
            && &self.object_identity_value(instance_id) == captured)
            .then(|| instance_id.to_string())
    }

    /// The current incarnation of an object on the battlefield or in another
    /// zone, if it has one (CR 400.7).
    fn current_object_incarnation(&self, instance_id: &str) -> Option<String> {
        self.permanent_position(instance_id)
            .map(|(player_index, card_index)| {
                &self.state.players[player_index].battlefield[card_index]
            })
            .or_else(|| self.card_outside_battlefield(instance_id))
            .and_then(Self::tracked_zone_identity)
            .map(str::to_string)
    }

    /// Records what each captured reference names when the delayed trigger
    /// is created: a player, or an object with its incarnation. A reference
    /// that names nothing (an illegal target, a spell that left the stack)
    /// or an object without a tracked incarnation is not captured: the
    /// delayed actions then find nothing for it.
    pub(super) fn capture_delayed_references(
        &self,
        captures: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Value {
        let mut captured = serde_json::Map::new();
        for capture in captures.as_array().into_iter().flatten() {
            let Some(binding_id) = capture["binding"].as_str() else {
                continue;
            };
            let reference = &capture["reference"];
            let target = match value_kind(reference) {
                Some("triggeringPlayer") => self
                    .resolve_player(reference, stack_object, bindings)
                    .map(|player_id| TargetRef::Player { player_id }),
                // The source of the creating ability (CR 603.7d).
                Some("self") => {
                    let instance_id = stack_object.card.instance_id.clone();
                    if self.permanent_position(&instance_id).is_some() {
                        Some(TargetRef::Permanent { instance_id })
                    } else {
                        Some(TargetRef::Card { instance_id })
                    }
                }
                _ => self.resolve_target(reference, stack_object, bindings),
            };
            let record = match target {
                Some(TargetRef::Player { player_id }) => Some(json!({ "player": player_id })),
                Some(TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id }) => {
                    self.current_object_incarnation(&instance_id)
                        .map(|identity| json!({ "object": instance_id, "identity": identity }))
                }
                Some(TargetRef::StackObject { .. }) | None => None,
            };
            if let Some(record) = record {
                captured.insert(binding_id.to_string(), record);
            }
        }
        Value::Object(captured)
    }

    /// The bindings a resolving delayed trigger starts with: each captured
    /// player, and each captured object while it is still the same object
    /// (CR 603.7c, 400.7). Any other ability starts with no binding.
    pub(super) fn captured_reference_bindings(
        &self,
        stack_object: &StackObject,
    ) -> BTreeMap<String, RuntimeBinding> {
        let mut bindings = BTreeMap::new();
        let Some(captured) = stack_object
            .decisions
            .get(CAPTURED_REFERENCES_KEY)
            .and_then(Value::as_object)
        else {
            return bindings;
        };
        for (binding_id, record) in captured {
            if let Some(player_id) = record["player"].as_str() {
                bindings.insert(
                    binding_id.clone(),
                    RuntimeBinding::Player(player_id.to_string()),
                );
                continue;
            }
            let Some(instance_id) = record["object"].as_str() else {
                continue;
            };
            if record["identity"].as_str().is_none()
                || self.current_object_incarnation(instance_id).as_deref()
                    != record["identity"].as_str()
            {
                continue;
            }
            let target = if self.permanent_position(instance_id).is_some() {
                TargetRef::Permanent {
                    instance_id: instance_id.to_string(),
                }
            } else {
                TargetRef::Card {
                    instance_id: instance_id.to_string(),
                }
            };
            bindings.insert(binding_id.clone(), RuntimeBinding::Target(target));
        }
        bindings
    }
}
