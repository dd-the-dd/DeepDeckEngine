//! The cards a card-batch event record names (`triggeringCardIds`): a
//! graveyard arrival publishes the cards it moved, one per occurrence or the
//! whole simultaneous batch (CR 603.2c). `{"kind":"triggeringCard"}` reads
//! them where the event put them, only while each is still the object the
//! event saw (CR 400.7); a card that left that zone, or came back as a new
//! object, is no longer named.
use super::*;

const EVENT_CARD_IDS: &str = "triggeringCardIds";
const EVENT_CARD_INCARNATIONS: &str = "triggeringCardIncarnations";

impl GameEngine {
    /// Captures, as the ability is put on the stack, the public zone and the
    /// incarnation of every card the record names. A record that already
    /// carries them keeps its own values; a card outside a graveyard or
    /// exile has no public incarnation to follow.
    pub(super) fn capture_event_card_batch(&self, decisions: &mut BTreeMap<String, Value>) {
        if decisions.contains_key(EVENT_CARD_INCARNATIONS) {
            return;
        }
        let Some(ids) = decisions
            .get(EVENT_CARD_IDS)
            .and_then(Value::as_array)
            .cloned()
        else {
            return;
        };
        let mut incarnations = serde_json::Map::new();
        for id in ids.iter().filter_map(Value::as_str) {
            if let Ok(Some(source)) = self.prepare_card_movement_source(id, None)
                && matches!(source.zone.name(), "graveyard" | "exile")
                && !source.identity.zone_identity.is_empty()
            {
                incarnations.insert(
                    id.to_string(),
                    json!({
                        "zone": source.zone.name(),
                        "identity": source.identity.zone_identity,
                    }),
                );
            }
        }
        decisions.insert(
            EVENT_CARD_INCARNATIONS.to_string(),
            Value::Object(incarnations),
        );
    }

    /// The cards of the record that are still the incarnation it captured,
    /// in the record's order.
    pub(super) fn event_card_batch_ids(&self, stack_object: &StackObject) -> Vec<String> {
        let decisions = &stack_object.decisions;
        let (Some(ids), Some(incarnations)) = (
            decisions.get(EVENT_CARD_IDS).and_then(Value::as_array),
            decisions
                .get(EVENT_CARD_INCARNATIONS)
                .and_then(Value::as_object),
        ) else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(Value::as_str)
            .filter(|id| {
                let Some(captured) = incarnations.get(*id) else {
                    return false;
                };
                match self.prepare_card_movement_source(id, None) {
                    Ok(Some(source)) => {
                        captured["zone"].as_str() == Some(source.zone.name())
                            && captured["identity"].as_str()
                                == Some(source.identity.zone_identity.as_str())
                    }
                    _ => false,
                }
            })
            .map(str::to_string)
            .collect()
    }
}
