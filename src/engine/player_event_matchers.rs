//! Matchers of player keyword-action and outcome events: committing a crime
//! (CR 700.13), rolling a die (CR 706), flipping a coin (CR 705), clashing
//! (CR 701.30), proliferating (CR 701.34) and exerting a permanent
//! (CR 701.43), plus the mana-ability observer of a tapped permanent with a
//! written player relation (CR 106.11a, 605.1a).
//!
//! Each occurrence is one event of its acting player. The observing rule's
//! `player` reference is matched against that player (never assumed to be
//! the observer's controller) and the acting player is bound as the
//! triggering player (`that player`). Each event kind keeps its own typed
//! attributes (a die-result range, a flip outcome, a won clash, the exerted
//! permanent's predicate); one publication routine serves every kind.
use super::*;

/// The `crimeCommitted` record of the action log (CR 700.13).
pub(super) const CRIME_COMMITTED_EVENT: &str = "crimeCommitted";

fn has_only_fields(event: &Value, fields: &[&str]) -> bool {
    event
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
}

fn positive_bound(event: &Value, field: &str) -> bool {
    event
        .get(field)
        .is_none_or(|value| integer_value(value).is_some_and(|value| value >= 1))
}

/// Support of the player-event kinds this module publishes, and of the
/// first-occurrence modifier on any event. `None` for any other kind.
pub(super) fn player_outcome_event_supported(event: &Value) -> Option<bool> {
    if event.get("firstTimeEachTurn").is_some() {
        return Some(previous_turn_events::first_occurrence_event_supported(
            event,
        ));
    }
    if let Some(supported) = event_model::attackers_attacked_player_event_supported(event) {
        return Some(supported);
    }
    let player = || player_action_events::event_player_reference_supported(&event["player"]);
    let supported = match value_kind(event)? {
        "crimeCommitted" | "proliferated" => {
            has_only_fields(event, &["kind", "player"]) && player()
        }
        "dieRolled" => {
            has_only_fields(
                event,
                &["kind", "player", "minimum", "maximum", "aggregation"],
            ) && player()
                && positive_bound(event, "minimum")
                && positive_bound(event, "maximum")
                && event.get("aggregation").is_none_or(|aggregation| {
                    aggregation.as_str() == Some("oneOrMore")
                        && event.get("minimum").is_none()
                        && event.get("maximum").is_none()
                })
        }
        "coinFlipped" => {
            has_only_fields(event, &["kind", "player", "outcome"])
                && player()
                && matches!(event["outcome"].as_str(), Some("won" | "lost"))
        }
        "clashed" => {
            has_only_fields(event, &["kind", "player", "won"])
                && player()
                && event.get("won").is_none_or(Value::is_boolean)
        }
        "permanentExerted" => {
            has_only_fields(event, &["kind", "player", "where"])
                && player()
                && event.get("where").is_none_or(card_filter_supported)
        }
        // CR 724.1: the player became the monarch.
        "becameMonarch" => has_only_fields(event, &["kind", "player"]) && player(),
        // CR 701.23: the player searched their library. The historical
        // `anyPlayer` form has no player relation and keeps its own matcher.
        "librarySearched" if event.get("player").is_some() => {
            has_only_fields(event, &["kind", "player"]) && player()
        }
        // CR 122.1: one instruction put counters of `counter` on the player.
        "playerCountersAdded" => {
            has_only_fields(event, &["kind", "player", "counter"])
                && player()
                && event["counter"]
                    .as_str()
                    .is_some_and(|counter| !counter.is_empty())
        }
        _ => return None,
    };
    Some(supported)
}

/// `playerDealtDamage` (CR 120.3): damage dealt to a player in a written
/// relation to the observer (`you`, `a player`), per damaged player or as
/// one `oneOrMore` event, optionally restricted to combat or noncombat
/// damage. `opponentDealtDamage` keeps the established opponent observer.
pub(super) fn received_damage_event_supported(event: &Value) -> bool {
    has_only_fields(
        event,
        &[
            "kind",
            "player",
            "aggregation",
            "combatOnly",
            "noncombatOnly",
        ],
    ) && player_action_events::event_player_reference_supported(&event["player"])
        && event
            .get("aggregation")
            .is_none_or(|value| matches!(value.as_str(), Some("perRecipient" | "oneOrMore")))
        && event.get("combatOnly").is_none_or(Value::is_boolean)
        && event.get("noncombatOnly").is_none_or(Value::is_boolean)
        && !(event["combatOnly"].as_bool() == Some(true)
            && event["noncombatOnly"].as_bool() == Some(true))
}

/// `cardsDiscarded` (CR 701.9, 603.2c): one discard action of a player in
/// a written relation (one resolving instruction, one cost) that discarded
/// at least one card matching `where`.
pub(super) fn discard_batch_event_supported(event: &Value) -> bool {
    has_only_fields(event, &["kind", "player", "where", "excludeSource"])
        && player_action_events::event_player_reference_supported(&event["player"])
        && event.get("where").is_none_or(card_filter_supported)
        && event.get("excludeSource").is_none_or(Value::is_boolean)
}

/// `selfDiscarded`: the observing card itself was discarded, optionally
/// caused by a resolving spell or ability with a controller relation
/// (`cause`).
pub(super) fn self_discard_event_supported(event: &Value) -> bool {
    has_only_fields(event, &["kind", "object", "cause"])
        && value_kind(&event["object"]) == Some("self")
        && event.get("cause").is_none_or(|cause| {
            has_only_fields(cause, &["kind", "controller"])
                && value_kind(cause) == Some("resolvingStackObject")
                && cause
                    .get("controller")
                    .is_none_or(player_action_events::event_player_reference_supported)
        })
}

/// Whether an activation observer's player relation admits the activating
/// player (CR 602.2): an opponent of the observer's controller, any player
/// (`eachPlayer`), else the observer's controller.
pub(super) fn activation_player_observed(
    event: &Value,
    source: &CardInstance,
    activating_player_id: &str,
) -> bool {
    if event["opponentOfSourceController"].as_bool() == Some(true) {
        source.controller != activating_player_id
    } else if value_kind(&event["player"]) == Some("eachPlayer") {
        true
    } else {
        source.controller == activating_player_id
    }
}

/// `spellCounteredBy` (CR 701.6): a spell or ability whose controller
/// satisfies `controller` countered a spell matching `where`.
pub(super) fn spell_countered_event_supported(event: &Value) -> bool {
    has_only_fields(event, &["kind", "controller", "where"])
        && player_action_events::event_player_reference_supported(&event["controller"])
        && event.get("where").is_none_or(card_filter_supported)
}

/// Whether a damage kind restriction admits one damage record (CR 120.2).
fn damage_kind_admits(event: &Value, combat: bool) -> bool {
    (!combat || event["noncombatOnly"].as_bool() != Some(true))
        && (combat || event["combatOnly"].as_bool() != Some(true))
}

/// Whether a die-roll pattern observes one die showing `result` (CR 706.2).
fn die_result_matches(event: &Value, result: i32) -> bool {
    event.get("aggregation").is_none()
        && event
            .get("minimum")
            .and_then(integer_value)
            .is_none_or(|minimum| result >= minimum)
        && event
            .get("maximum")
            .and_then(integer_value)
            .is_none_or(|maximum| result <= maximum)
}

impl GameEngine {
    /// Battlefield observers of one player event of `kind` performed by
    /// `acting_player_id` whose own attributes satisfy `matches`. Each one
    /// triggers with the acting player bound as the triggering player, and
    /// with the event's permanent when it has one.
    pub(super) fn publish_player_outcome_event(
        &mut self,
        kind: &str,
        acting_player_id: &str,
        permanent_id: Option<&str>,
        matches: impl Fn(&Value) -> bool,
    ) {
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_active_for_source(source, rule)
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    value_kind(event) == Some(kind)
                                        && self.event_player_reference_matches(
                                            &event["player"],
                                            source,
                                            acting_player_id,
                                        )
                                        && matches(*event)
                                })
                    })
                    .cloned()
                    .map(|rule| (source.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut decisions = BTreeMap::from([(
            "triggeringPlayerId".to_string(),
            Value::String(acting_player_id.to_string()),
        )]);
        if let Some(permanent_id) = permanent_id {
            decisions.insert(
                "triggeringPermanentId".to_string(),
                Value::String(permanent_id.to_string()),
            );
        }
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// CR 700.13: a player commits a crime as they cast a spell, activate
    /// an ability or put a triggered ability on the stack that targets an
    /// opponent, a permanent or a spell or ability an opponent controls, or
    /// a card in an opponent's graveyard. One crime per stack object,
    /// however many such targets it has. Called once its targets are
    /// committed; a later change of targets commits no crime.
    pub(super) fn publish_committed_crime(
        &mut self,
        stack_id: &str,
        targets: &BTreeMap<String, TargetRef>,
    ) {
        let Some(player_id) = self
            .state
            .stack
            .iter()
            .find(|object| object.id == stack_id)
            .map(|object| object.controller.clone())
        else {
            return;
        };
        if !targets
            .values()
            .any(|target| self.target_is_crime(&player_id, target))
        {
            return;
        }
        self.record_event(
            CRIME_COMMITTED_EVENT,
            Some(player_id.clone()),
            None,
            json!({ "stackId": stack_id }),
        );
        self.publish_player_outcome_event(CRIME_COMMITTED_EVENT, &player_id, None, |_| true);
    }

    /// Whether targeting `target` is a crime of `player_id` (CR 700.13).
    fn target_is_crime(&self, player_id: &str, target: &TargetRef) -> bool {
        match target {
            TargetRef::Player { player_id: target } => target != player_id,
            TargetRef::Permanent { instance_id } => self
                .permanent_position(instance_id)
                .is_some_and(|(player_index, card_index)| {
                    self.state.players[player_index].battlefield[card_index].controller != player_id
                }),
            TargetRef::StackObject { stack_id } => self
                .state
                .stack
                .iter()
                .find(|object| object.id == *stack_id)
                .is_some_and(|object| object.controller != player_id),
            TargetRef::Card { instance_id } => self.state.players.iter().any(|player| {
                player.id != player_id
                    && player
                        .graveyard
                        .iter()
                        .any(|card| card.instance_id == *instance_id)
            }),
        }
    }

    /// One die of `player_id` showing `result` (CR 706.2): per-die
    /// observers whose result range contains it.
    pub(super) fn publish_die_rolled(&mut self, player_id: &str, result: i32) {
        self.publish_player_outcome_event("dieRolled", player_id, None, |event| {
            die_result_matches(event, result)
        });
    }

    /// One roll instruction of `player_id` that rolled at least one die:
    /// `one or more dice` observers trigger once (CR 603.2c).
    pub(super) fn publish_dice_rolled_batch(&mut self, player_id: &str) {
        self.publish_player_outcome_event("dieRolled", player_id, None, |event| {
            event["aggregation"].as_str() == Some("oneOrMore")
        });
    }

    /// One coin flip `player_id` won or lost (CR 705.2).
    pub(super) fn publish_coin_flipped(&mut self, player_id: &str, won: bool) {
        let outcome = if won { "won" } else { "lost" };
        self.publish_player_outcome_event("coinFlipped", player_id, None, |event| {
            event["outcome"].as_str() == Some(outcome)
        });
    }

    /// One clash (CR 701.30): every clashing player clashed; a `won`
    /// observer requires that player to be the winner (CR 701.30d).
    pub(super) fn publish_clashed(&mut self, clashers: &[String], winner: Option<&str>) {
        for player_id in clashers {
            let won = winner == Some(player_id.as_str());
            self.publish_player_outcome_event("clashed", player_id, None, |event| {
                event["won"].as_bool() != Some(true) || won
            });
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// `player_id` became the monarch (CR 724.1).
    pub(super) fn publish_became_monarch(&mut self, player_id: &str) {
        self.publish_player_outcome_event("becameMonarch", player_id, None, |_| true);
    }

    /// `player_id` searched their library (CR 701.23): observers with a
    /// player relation. The historical `anyPlayer` observer has no `player`
    /// and is never matched here.
    pub(super) fn publish_library_searched(&mut self, player_id: &str) {
        self.publish_player_outcome_event("librarySearched", player_id, None, |_| true);
    }

    /// One instruction put at least one `counter` counter on `player_id`
    /// (CR 122.1, 603.2c): `get one or more {E}` observers of that counter.
    pub(super) fn publish_player_counters_added(&mut self, player_id: &str, counter: &str) {
        self.publish_player_outcome_event("playerCountersAdded", player_id, None, |event| {
            event["counter"].as_str() == Some(counter)
        });
    }

    /// One proliferation of `player_id` (CR 701.34).
    pub(super) fn publish_proliferated(&mut self, player_id: &str) {
        self.publish_player_outcome_event("proliferated", player_id, None, |_| true);
    }

    /// `player_id` exerted `permanent_id` (CR 701.43): observers whose
    /// permanent predicate the exerted permanent satisfies.
    pub(super) fn publish_permanent_exerted(&mut self, player_id: &str, permanent_id: &str) {
        let Some(definition) =
            self.permanent_position(permanent_id)
                .map(|(player_index, card_index)| {
                    self.state.players[player_index].battlefield[card_index]
                        .definition
                        .clone()
                })
        else {
            return;
        };
        self.publish_player_outcome_event(
            "permanentExerted",
            player_id,
            Some(permanent_id),
            |event| {
                event
                    .get("where")
                    .is_none_or(|filter| matches_card_filter(&definition, filter))
            },
        );
    }

    /// One discarded card of `player_id`: inside an open simultaneous event
    /// (a resolving instruction, CR 603.2c) it joins that event's discard;
    /// outside any event (a cost, cleanup) it is its own discard action.
    pub(super) fn observe_discard_for_batch(&mut self, player_id: &str, card_id: &str) {
        match self.event_batches.current_batch() {
            Some(batch) => self.event_batches.defer_discard(batch, player_id, card_id),
            None => self.publish_cards_discarded(player_id, &[card_id.to_string()]),
        }
    }

    /// The discards of an ended simultaneous event: one discard action per
    /// discarding player, in first-discard order.
    pub(super) fn flush_discard_batch(&mut self, batch: u64) {
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for (player_id, card_id) in self.event_batches.take_discards(batch) {
            match groups.iter_mut().find(|(player, _)| *player == player_id) {
                Some((_, cards)) => cards.push(card_id),
                None => groups.push((player_id, vec![card_id])),
            }
        }
        for (player_id, card_ids) in groups {
            self.publish_cards_discarded(&player_id, &card_ids);
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// One discard action of `player_id` (CR 701.9): `one or more` observers
    /// whose player relation matches trigger once when at least one of the
    /// discarded cards matches, with the matching cards' number as the
    /// event's object count (`that many`).
    fn publish_cards_discarded(&mut self, player_id: &str, card_ids: &[String]) {
        let cards = card_ids
            .iter()
            .filter_map(|card_id| self.card_after_player_action(card_id))
            .collect::<Vec<_>>();
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule) && self.rule_active_for_source(source, rule)
                    })
                    .filter_map(|rule| {
                        let count = triggered_event_variants(&rule["event"])
                            .into_iter()
                            .filter(|event| {
                                value_kind(event) == Some("cardsDiscarded")
                                    && self.event_player_reference_matches(
                                        &event["player"],
                                        source,
                                        player_id,
                                    )
                            })
                            .map(|event| {
                                cards
                                    .iter()
                                    .filter(|card| {
                                        self.player_card_action_matches(
                                            event,
                                            source,
                                            player_id,
                                            Some(*card),
                                        )
                                    })
                                    .count()
                            })
                            .max()
                            .unwrap_or(0);
                        (count > 0).then(|| (source.clone(), rule.clone(), count))
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (source, rule, count) in triggers {
            let decisions = BTreeMap::from([
                (
                    "triggeringPlayerId".to_string(),
                    Value::String(player_id.to_string()),
                ),
                ("eventObjectCount".to_string(), Value::from(count as u64)),
            ]);
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// The controller of the resolving spell or ability whose effect made
    /// `player_id` discard their latest discarded card; `None` for a cost,
    /// a cleanup discard or any other discard without a recorded cause.
    fn latest_discard_cause_controller(&self, player_id: &str) -> Option<String> {
        self.state
            .events
            .iter()
            .rev()
            .find(|event| {
                event.kind == "cardDiscarded" && event.player_id.as_deref() == Some(player_id)
            })
            .filter(|event| event.detail["reason"].as_str() == Some("ability"))
            .and_then(|event| event.detail["sourceControllerId"].as_str())
            .map(str::to_string)
    }

    /// `When you discard this card` (CR 701.9): the discarded card's own
    /// `selfDiscarded` rules trigger from where the discard put it. A rule
    /// with a cause triggers only when a resolving spell or ability whose
    /// controller satisfies the relation caused the discard, and binds that
    /// controller as the triggering player (`that player`); otherwise the
    /// discarding player is bound. False when a quantity failure stops the
    /// publication.
    pub(super) fn enqueue_self_discard_triggers(
        &mut self,
        discarded: &CardInstance,
        discarding_player_id: &str,
    ) -> bool {
        let cause_controller = self.latest_discard_cause_controller(discarding_player_id);
        let triggers = self
            .effective_rules(discarded)
            .iter()
            .filter(|rule| triggered_rule_supported(rule))
            .filter_map(|rule| {
                triggered_event_variants(&rule["event"])
                    .into_iter()
                    .filter(|event| value_kind(event) == Some("selfDiscarded"))
                    .find_map(|event| match event.get("cause") {
                        None => Some(discarding_player_id.to_string()),
                        Some(cause) => cause_controller.clone().filter(|controller_id| {
                            cause.get("controller").is_none_or(|relation| {
                                self.event_player_reference_matches(
                                    relation,
                                    discarded,
                                    controller_id,
                                )
                            })
                        }),
                    })
                    .map(|triggering_player| (rule.clone(), triggering_player))
            })
            .collect::<Vec<_>>();
        for (rule, triggering_player) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(
                discarded,
                rule,
                BTreeMap::from([(
                    "triggeringPlayerId".to_string(),
                    Value::String(triggering_player),
                )]),
            );
            if self.check_quantity_failure().is_err() {
                return false;
            }
        }
        true
    }

    /// A spell or ability of `countering_player_id` countered a spell with
    /// `countered` as its definition (CR 701.6): observers whose controller
    /// relation and spell predicate match trigger, with the countering
    /// player bound as the triggering player.
    pub(super) fn publish_spell_countered(
        &mut self,
        countering_player_id: &str,
        countered: &CardDefinition,
    ) {
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_active_for_source(source, rule)
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    value_kind(event) == Some("spellCounteredBy")
                                        && self.event_player_reference_matches(
                                            &event["controller"],
                                            source,
                                            countering_player_id,
                                        )
                                        && event.get("where").is_none_or(|filter| {
                                            matches_card_filter(countered, filter)
                                        })
                                })
                    })
                    .cloned()
                    .map(|rule| (source.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(
                &source,
                rule,
                BTreeMap::from([(
                    "triggeringPlayerId".to_string(),
                    Value::String(countering_player_id.to_string()),
                )]),
            );
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// `playerDealtDamage` observed by `source` for damage dealt to
    /// `damaged_player_id` (CR 120.3): the damaged player satisfies the
    /// written relation and the damage kind is admitted.
    pub(super) fn received_damage_observed(
        &self,
        event: &Value,
        source: &CardInstance,
        damaged_player_id: &str,
        combat: bool,
    ) -> bool {
        self.event_player_reference_matches(&event["player"], source, damaged_player_id)
            && damage_kind_admits(event, combat)
    }

    /// `controlledPermanentManaAbilityActivated` observed by `source` for a
    /// mana ability of `mana_source` (CR 106.11a): the tapped permanent's
    /// controller is the acting player, matched by the written relation
    /// (`you`, `an opponent`) or by any player; the permanent satisfies the
    /// shared filter.
    pub(super) fn tapped_for_mana_observed(
        &self,
        event: &Value,
        source: &CardInstance,
        mana_source: &CardInstance,
    ) -> bool {
        let player_matches = event["anyPlayer"].as_bool() == Some(true)
            || match event.get("player") {
                Some(player) => {
                    self.event_player_reference_matches(player, source, &mana_source.controller)
                }
                None => source.controller == mana_source.controller,
            };
        player_matches && matches_card_filter(&mana_source.definition, &event["where"])
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "player_event_matchers_tests.rs"]
mod tests;
