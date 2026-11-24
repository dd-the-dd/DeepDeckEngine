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

/// Player special-action and spending events of this family: `roomFullyUnlocked`
/// (CR 709.5) and `manaExpended` (CR 701.58). `None` for other kinds.
pub(super) fn player_milestone_event_supported(event: &Value) -> Option<bool> {
    match value_kind(event)? {
        "roomFullyUnlocked" => Some(
            event.as_object().is_some_and(|object| {
                object
                    .keys()
                    .all(|key| matches!(key.as_str(), "kind" | "player"))
            }) && event_player_reference_supported(&event["player"]),
        ),
        "manaExpended" => Some(
            event.as_object().is_some_and(|object| {
                object
                    .keys()
                    .all(|key| matches!(key.as_str(), "kind" | "player" | "threshold"))
            }) && event_player_reference_supported(&event["player"])
                && integer_value(&event["threshold"]).is_some_and(|threshold| threshold > 0),
        ),
        _ => None,
    }
}

impl GameEngine {
    /// Whether one player event of `event_kind` performed by `acting_player_id`
    /// is observed by `event` of `source`: a written player relation is matched
    /// against the acting player, never assumed to be the observer's controller;
    /// an event without a player keeps the historical controller-only meaning.
    fn player_event_observed(
        &self,
        event: &Value,
        event_kind: &str,
        source: &CardInstance,
        acting_player_id: &str,
        event_object_id: Option<&str>,
    ) -> bool {
        value_kind(event) == Some(event_kind)
            && match event.get("player") {
                Some(player) if event_player_reference_supported(player) => {
                    self.event_player_reference_matches(player, source, acting_player_id)
                }
                _ => source.controller == acting_player_id,
            }
            && !(event["excludeSource"].as_bool().unwrap_or(false)
                && event_object_id == Some(source.instance_id.as_str()))
    }

    /// Battlefield observers of one player event (`landPlayed`, ...), each
    /// with the acting player bound as the triggering player (`that player`).
    pub(super) fn enqueue_observed_player_event_triggers(
        &mut self,
        event_kind: &str,
        acting_player_id: &str,
        event_object_id: Option<&str>,
    ) {
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
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    self.player_event_observed(
                                        event,
                                        event_kind,
                                        source,
                                        acting_player_id,
                                        event_object_id,
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
            Value::String(acting_player_id.to_string()),
        )]);
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// CR 709.5: after a door of `room` is unlocked, a Room with every door
    /// unlocked was fully unlocked by its controller. Observers of
    /// `roomFullyUnlocked` whose player relation matches that player trigger.
    pub(super) fn enqueue_room_fully_unlocked_triggers(&mut self, room: &CardInstance) {
        let fully_unlocked = (0..2).all(|door_index| {
            room.flags
                .get(&format!("roomDoor{door_index}Unlocked"))
                .copied()
                .unwrap_or(false)
        });
        if !fully_unlocked {
            return;
        }
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
                                    value_kind(event) == Some("roomFullyUnlocked")
                                        && self.event_player_reference_matches(
                                            &event["player"],
                                            source,
                                            &room.controller,
                                        )
                                })
                    })
                    .cloned()
                    .map(|rule| (source.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let decisions = BTreeMap::from([
            (
                "triggeringPlayerId".to_string(),
                Value::String(room.controller.clone()),
            ),
            (
                "triggeringPermanentId".to_string(),
                Value::String(room.instance_id.clone()),
            ),
        ]);
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// CR 701.58: a player expends N as they spend their Nth total mana to
    /// cast spells during a turn. Called once the cast is logged: every
    /// threshold crossed by this spell's mana triggers its observers once.
    pub(super) fn enqueue_mana_expended_triggers(
        &mut self,
        caster_id: &str,
        cast_decisions: &BTreeMap<String, Value>,
    ) {
        let spent = i64::from(mana_spent_from_cast_decisions(cast_decisions).max(0));
        if spent == 0 {
            return;
        }
        // The current spell is already logged: the turn total includes it.
        let total: i64 = self
            .state
            .events
            .iter()
            .filter(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == "spellCast"
                    && event.player_id.as_deref() == Some(caster_id)
            })
            .map(|event| {
                serde_json::from_value::<BTreeMap<String, Value>>(event.detail["decisions"].clone())
                    .map(|decisions| i64::from(mana_spent_from_cast_decisions(&decisions).max(0)))
                    .unwrap_or(0)
            })
            .sum();
        let before = total - spent;
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
                                    value_kind(event) == Some("manaExpended")
                                        && integer_value(&event["threshold"]).is_some_and(
                                            |threshold| {
                                                before < i64::from(threshold)
                                                    && i64::from(threshold) <= total
                                            },
                                        )
                                        && self.event_player_reference_matches(
                                            &event["player"],
                                            source,
                                            caster_id,
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
            Value::String(caster_id.to_string()),
        )]);
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

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

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "trigger_event_language_tests.rs"]
mod trigger_event_language_tests;
