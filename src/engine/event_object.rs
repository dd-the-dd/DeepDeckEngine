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
const EVENT_OBJECT_POWER: &str = "triggeringPermanentPower";
const EVENT_OBJECT_TOUGHNESS: &str = "triggeringPermanentToughness";
const EVENT_OBJECT_MANA_VALUE: &str = "triggeringPermanentManaValue";
const EVENT_CARD_ZONE: &str = "triggeringCardZone";
const EVENT_CARD_IDENTITY: &str = "triggeringCardIdentity";

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
            // The card the object became in a public zone (CR 400.7): the
            // ability can find it there while it stays that object.
            if let Ok(Some(source)) = self.prepare_card_movement_source(&instance_id, None)
                && matches!(source.zone.name(), "graveyard" | "exile")
                && !source.identity.zone_identity.is_empty()
            {
                decisions.insert(
                    EVENT_CARD_ZONE.to_string(),
                    Value::String(source.zone.name().to_string()),
                );
                decisions.insert(
                    EVENT_CARD_IDENTITY.to_string(),
                    Value::String(source.identity.zone_identity),
                );
            }
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
        for (key, value) in self.event_object_characteristics(live) {
            decisions.entry(key.to_string()).or_insert(value);
        }
    }

    /// The characteristics a later read of the event object needs once it is
    /// gone: power, toughness and mana value as they last existed.
    fn event_object_characteristics(&self, card: &CardInstance) -> [(&'static str, Value); 3] {
        [
            (EVENT_OBJECT_POWER, Value::from(self.current_power(card))),
            (
                EVENT_OBJECT_TOUGHNESS,
                Value::from(self.current_toughness(card)),
            ),
            (
                EVENT_OBJECT_MANA_VALUE,
                Value::from(mana_value(&card.definition)),
            ),
        ]
    }

    /// Refreshes the last known controller and owner of every pending event
    /// object that is this departing incarnation (CR 608.2h).
    pub(super) fn refresh_event_object_lki(&mut self, departing: &CardInstance) {
        let Some(identity) = Self::tracked_zone_identity(departing).map(str::to_string) else {
            return;
        };
        let characteristics = self.event_object_characteristics(departing);
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
                for (key, value) in &characteristics {
                    object.decisions.insert(key.to_string(), value.clone());
                }
            }
        }
    }

    /// `{"kind":"triggeringCard"}`: the card the event's object became in a
    /// public zone, while it is still there as that object.
    pub(super) fn event_card_ids(&self, stack_object: &StackObject) -> Vec<String> {
        let decisions = &stack_object.decisions;
        let (Some(instance_id), Some(zone), Some(identity)) = (
            decisions.get(EVENT_OBJECT_ID).and_then(Value::as_str),
            decisions.get(EVENT_CARD_ZONE).and_then(Value::as_str),
            decisions.get(EVENT_CARD_IDENTITY).and_then(Value::as_str),
        ) else {
            return Vec::new();
        };
        match self.prepare_card_movement_source(instance_id, None) {
            Ok(Some(source))
                if source.zone.name() == zone && source.identity.zone_identity == identity =>
            {
                vec![instance_id.to_string()]
            }
            _ => Vec::new(),
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

    /// The live event object (the record's permanent or the observed entering
    /// permanent) while it is the incarnation the event saw.
    fn event_object_live(
        &self,
        reference: &Value,
        stack_object: &StackObject,
    ) -> Option<&CardInstance> {
        match value_kind(reference) {
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
        .map(|(player, index)| &self.state.players[player].battlefield[index])
    }

    /// `powerOf` / `toughnessOf` / `manaValueOf` an event object: live, else
    /// its last known value. `None` when the expression is another property.
    pub(super) fn event_object_numeric(
        &self,
        expression: &Value,
        stack_object: &StackObject,
    ) -> Option<Option<i32>> {
        let key = match value_kind(expression)? {
            "powerOf" => EVENT_OBJECT_POWER,
            "toughnessOf" => EVENT_OBJECT_TOUGHNESS,
            "manaValueOf" => EVENT_OBJECT_MANA_VALUE,
            _ => return None,
        };
        if let Some(card) = self.event_object_live(&expression["object"], stack_object) {
            return Some(Some(match key {
                EVENT_OBJECT_POWER => self.current_power(card),
                EVENT_OBJECT_TOUGHNESS => self.current_toughness(card),
                _ => mana_value(&card.definition),
            }));
        }
        Some(
            stack_object
                .decisions
                .get(key)
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok()),
        )
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
        if let Some(card) = self.event_object_live(reference, stack_object) {
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
