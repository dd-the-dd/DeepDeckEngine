//! Player-action trigger events: discarding (CR 701.9), cycling (CR 702.29)
//! and completed named keyword actions (CR 701.22d). The acting player and
//! the card are separate attributes of one occurrence; the observing rule's
//! player reference is matched against the acting player, never assumed to
//! be the observer's controller.
use super::*;

/// Event player references whose meaning relative to the observing source is
/// decided by `event_player_reference_matches`.
pub(super) fn event_player_reference_supported(reference: &Value) -> bool {
    match value_kind(reference) {
        Some("abilityController" | "eachPlayer") => true,
        Some("controllerOf") => matches!(
            value_kind(&reference["object"]),
            Some("self" | "abilitySource")
        ),
        Some("opponentsOf") => event_player_reference_supported(&reference["player"]),
        Some("controllerOfAttachedPermanent") => {
            value_kind(&reference["attachment"]) == Some("self")
        }
        _ => false,
    }
}

/// `playerDiscardedCard` keeps its historical any-player meaning when it has
/// no `player`; `cardCycled` always names its acting player.
pub(super) fn player_card_action_event_supported(event: &Value) -> bool {
    let player_supported = match event.get("player") {
        None => value_kind(event) == Some("playerDiscardedCard"),
        Some(player) => event_player_reference_supported(player),
    };
    player_supported
        && event.get("where").is_none_or(card_filter_supported)
        && event.get("excludeSource").is_none_or(Value::is_boolean)
}

impl GameEngine {
    /// One occurrence of a player acting on a card, observed by `source`.
    pub(super) fn player_card_action_matches(
        &self,
        event: &Value,
        source: &CardInstance,
        acting_player_id: &str,
        card: Option<&CardInstance>,
    ) -> bool {
        event.get("player").is_none_or(|player| {
            self.event_player_reference_matches(player, source, acting_player_id)
        }) && event.get("where").is_none_or(|filter| {
            card.is_some_and(|card| matches_card_filter(&card.definition, filter))
        }) && !(event["excludeSource"].as_bool() == Some(true)
            && card.is_some_and(|card| card.instance_id == source.instance_id))
    }

    /// The card a player just put into a graveyard or exile, wherever a
    /// replacement sent it.
    pub(super) fn card_after_player_action(&self, instance_id: &str) -> Option<CardInstance> {
        self.state
            .players
            .iter()
            .flat_map(|player| player.graveyard.iter().chain(player.exile.iter()))
            .find(|card| card.instance_id == instance_id)
            .cloned()
    }

    /// CR 702.29a: cycling's cost discards the card. Publish that discard
    /// and the cycling occurrence for observers of either event.
    pub(super) fn publish_cycling_card_actions(&mut self, player_id: &str, cycled: &CardInstance) {
        self.record_event(
            "cardDiscarded",
            Some(player_id.to_string()),
            Some(cycled.instance_id.clone()),
            json!({"reason": "cost", "cycling": true}),
        );
        self.enqueue_card_discarded_triggers(player_id);
        if self.check_quantity_failure().is_err() {
            return;
        }
        self.enqueue_card_cycled_triggers(player_id, cycled);
    }

    fn enqueue_card_cycled_triggers(&mut self, player_id: &str, cycled: &CardInstance) {
        let card = self
            .card_after_player_action(&cycled.instance_id)
            .unwrap_or_else(|| cycled.clone());
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(&source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_active_for_source(source, rule)
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    value_kind(event) == Some("cardCycled")
                                        && self.player_card_action_matches(
                                            event,
                                            source,
                                            player_id,
                                            Some(&card),
                                        )
                                })
                    })
                    .cloned()
                    .map(|rule| (source.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let decisions = BTreeMap::from([(
            "triggeringPlayerId".to_string(),
            Value::String(player_id.to_string()),
        )]);
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}
