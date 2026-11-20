//! `damageDealtBySource`: observers of a damage source dealing damage to one
//! recipient (CR 120.1, 120.2, 603.2c). The damage source, the damage kind and
//! the recipient are independent attributes of one damage event; every
//! damage path (combat, a single recipient, a player batch) publishes each
//! recipient's event through this one matcher.
use super::*;

/// The recipient of one dealt-damage event.
pub(super) enum DamageRecipientRef<'a> {
    Player(&'a str),
    Permanent(&'a CardInstance),
}

fn damage_permanent_query_supported(query: &Value) -> bool {
    query.get("where").is_none_or(card_filter_supported)
        && query
            .get("controller")
            .is_none_or(player_action_events::event_player_reference_supported)
}

pub(super) fn damage_dealt_by_source_event_supported(event: &Value) -> bool {
    let source = &event["source"];
    let source_supported = match value_kind(source) {
        Some("self") => true,
        Some("attachedPermanent") => value_kind(&source["attachment"]) == Some("self"),
        Some("permanents") => {
            damage_permanent_query_supported(source)
                && source.get("excludeSource").is_none_or(Value::is_boolean)
        }
        _ => false,
    };
    let recipient = &event["recipient"];
    let recipient_supported = match value_kind(recipient) {
        Some("player") => {
            player_action_events::event_player_reference_supported(&recipient["player"])
        }
        Some("permanent") => damage_permanent_query_supported(recipient),
        _ => false,
    };
    source_supported
        && recipient_supported
        && event.get("combatOnly").is_none_or(Value::is_boolean)
        && event.get("noncombatOnly").is_none_or(Value::is_boolean)
        && !(event["combatOnly"].as_bool() == Some(true)
            && event["noncombatOnly"].as_bool() == Some(true))
}

impl GameEngine {
    fn damage_permanent_query_matches(
        &self,
        query: &Value,
        observer: &CardInstance,
        permanent: &CardInstance,
    ) -> bool {
        query.get("controller").is_none_or(|controller| {
            self.event_player_reference_matches(controller, observer, &permanent.controller)
        }) && query
            .get("where")
            .is_none_or(|filter| self.permanent_matches_effective_filter(permanent, filter))
    }

    fn damage_source_matches(
        &self,
        source: &Value,
        observer: &CardInstance,
        damage_source_id: &str,
    ) -> bool {
        match value_kind(source) {
            Some("self") => observer.instance_id == damage_source_id,
            Some("attachedPermanent") => observer.attached_to.as_deref() == Some(damage_source_id),
            Some("permanents") => {
                !(source["excludeSource"].as_bool() == Some(true)
                    && observer.instance_id == damage_source_id)
                    && self.permanent_position(damage_source_id).is_some_and(
                        |(player_index, card_index)| {
                            self.damage_permanent_query_matches(
                                source,
                                observer,
                                &self.state.players[player_index].battlefield[card_index],
                            )
                        },
                    )
            }
            _ => false,
        }
    }

    /// Publishes one dealt-damage event to every matching observer.
    pub(super) fn enqueue_source_damage_observers(
        &mut self,
        damage_source_id: &str,
        recipient: DamageRecipientRef<'_>,
        amount: i32,
        combat: bool,
    ) {
        if amount <= 0 {
            return;
        }
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|observer| {
                observer
                    .definition
                    .rules
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_active_for_source(observer, rule)
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    value_kind(event) == Some("damageDealtBySource")
                                        && !(combat
                                            && event["noncombatOnly"].as_bool() == Some(true))
                                        && !(!combat && event["combatOnly"].as_bool() == Some(true))
                                        && self.damage_source_matches(
                                            &event["source"],
                                            observer,
                                            damage_source_id,
                                        )
                                        && match (&recipient, value_kind(&event["recipient"])) {
                                            (
                                                DamageRecipientRef::Player(player_id),
                                                Some("player"),
                                            ) => self.event_player_reference_matches(
                                                &event["recipient"]["player"],
                                                observer,
                                                player_id,
                                            ),
                                            (
                                                DamageRecipientRef::Permanent(permanent),
                                                Some("permanent"),
                                            ) => self.damage_permanent_query_matches(
                                                &event["recipient"],
                                                observer,
                                                permanent,
                                            ),
                                            _ => false,
                                        }
                                })
                    })
                    .cloned()
                    .map(|rule| (observer.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if triggers.is_empty() {
            return;
        }
        // `that player` names a damaged player; `that creature` names a
        // damaged permanent, otherwise the damage source (as for the
        // established combat-damage observers).
        let mut decisions = BTreeMap::from([
            ("damageAmount".to_string(), Value::from(amount)),
            (
                "damageSourceId".to_string(),
                Value::String(damage_source_id.to_string()),
            ),
        ]);
        match recipient {
            DamageRecipientRef::Player(player_id) => {
                decisions.insert(
                    "triggeringPlayerId".to_string(),
                    Value::String(player_id.to_string()),
                );
                decisions.insert(
                    "triggeringPermanentId".to_string(),
                    Value::String(damage_source_id.to_string()),
                );
            }
            DamageRecipientRef::Permanent(permanent) => {
                decisions.insert(
                    "triggeringPermanentId".to_string(),
                    Value::String(permanent.instance_id.clone()),
                );
            }
        }
        for (observer, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&observer, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}
