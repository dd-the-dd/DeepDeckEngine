//! The permanent a triggering event record binds (`triggeringPermanentId`):
//! the dealt-damage recipient, the attacker, the entering or dying permanent.
//! The reference `{"kind":"triggeringPermanent"}` denotes it only while it is
//! the incarnation the event saw (CR 400.7); its controller and owner are read
//! from the live object, else as last known information (CR 608.2h).
use super::*;

const EVENT_OBJECT_ID: &str = "triggeringPermanentId";
const EVENT_OBJECT_IDENTITY: &str = "triggeringPermanentIdentity";
const EVENT_OBJECT_CONTROLLER: &str = "triggeringPermanentController";
const EVENT_OBJECT_OWNER: &str = "triggeringPermanentOwner";

impl GameEngine {
    /// Captures the event object's incarnation, controller and owner when a
    /// triggered ability is put on the stack. A record that already carries
    /// them (a death's last known information) keeps its own values; an object
    /// that is no longer on the battlefield has no incarnation to find.
    pub(super) fn capture_event_object(&self, decisions: &mut BTreeMap<String, Value>) {
        let Some(instance_id) = decisions
            .get(EVENT_OBJECT_ID)
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if decisions.contains_key(EVENT_OBJECT_IDENTITY) {
            return;
        }
        let Some((player, index)) = self.permanent_position(&instance_id) else {
            decisions.insert(EVENT_OBJECT_IDENTITY.to_string(), Value::Null);
            return;
        };
        let live = &self.state.players[player].battlefield[index];
        // An object without a tracked incarnation keeps the historical
        // instance-only reading; no identity is minted for it.
        let Some(identity) = Self::tracked_zone_identity(live).filter(|value| !value.is_empty())
        else {
            return;
        };
        decisions.insert(
            EVENT_OBJECT_IDENTITY.to_string(),
            Value::String(identity.to_string()),
        );
        decisions
            .entry(EVENT_OBJECT_CONTROLLER.to_string())
            .or_insert_with(|| Value::String(live.controller.clone()));
        decisions
            .entry(EVENT_OBJECT_OWNER.to_string())
            .or_insert_with(|| Value::String(live.owner.clone()));
    }

    /// Refreshes the last known controller and owner of every pending event
    /// object that is this departing incarnation (CR 608.2h).
    pub(super) fn refresh_event_object_lki(&mut self, departing: &CardInstance) {
        let Some(identity) = Self::tracked_zone_identity(departing).map(str::to_string) else {
            return;
        };
        for object in &mut self.state.stack {
            if object
                .decisions
                .get(EVENT_OBJECT_ID)
                .and_then(Value::as_str)
                == Some(departing.instance_id.as_str())
                && object
                    .decisions
                    .get(EVENT_OBJECT_IDENTITY)
                    .and_then(Value::as_str)
                    == Some(identity.as_str())
            {
                object.decisions.insert(
                    EVENT_OBJECT_CONTROLLER.to_string(),
                    Value::String(departing.controller.clone()),
                );
                object.decisions.insert(
                    EVENT_OBJECT_OWNER.to_string(),
                    Value::String(departing.owner.clone()),
                );
            }
        }
    }

    /// The event object while it is the incarnation the event saw.
    pub(super) fn event_object_target(&self, stack_object: &StackObject) -> Option<TargetRef> {
        let instance_id = stack_object.decisions.get(EVENT_OBJECT_ID)?.as_str()?;
        let current = match stack_object.decisions.get(EVENT_OBJECT_IDENTITY) {
            // A record captured before incarnations were tracked.
            None => true,
            Some(Value::String(identity)) => {
                self.permanent_position(instance_id)
                    .map(|(player, index)| &self.state.players[player].battlefield[index])
                    .and_then(Self::tracked_zone_identity)
                    == Some(identity.as_str())
            }
            Some(_) => false,
        };
        current.then(|| TargetRef::Permanent {
            instance_id: instance_id.to_string(),
        })
    }

    /// The controller or owner of an event object (the record's permanent or
    /// the observed entering permanent): live while it is the same
    /// incarnation, else its last known information.
    pub(super) fn event_object_player(
        &self,
        relation: object_relation_lki::ObjectPlayerRelation,
        reference: &Value,
        stack_object: &StackObject,
    ) -> Option<String> {
        let live = match value_kind(reference) {
            Some("observedEventObject") => self
                .resolve_observed_event_permanent(reference, stack_object)
                .ok()
                .flatten(),
            _ => match self.event_object_target(stack_object) {
                Some(TargetRef::Permanent { instance_id }) => Some(instance_id),
                _ => None,
            },
        }
        .and_then(|instance_id| self.permanent_position(&instance_id))
        .map(|(player, index)| &self.state.players[player].battlefield[index]);
        if let Some(card) = live {
            return Some(match relation {
                object_relation_lki::ObjectPlayerRelation::Controller => card.controller.clone(),
                object_relation_lki::ObjectPlayerRelation::Owner => card.owner.clone(),
            });
        }
        let key = match relation {
            object_relation_lki::ObjectPlayerRelation::Controller => EVENT_OBJECT_CONTROLLER,
            object_relation_lki::ObjectPlayerRelation::Owner => EVENT_OBJECT_OWNER,
        };
        stack_object
            .decisions
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    }
}
