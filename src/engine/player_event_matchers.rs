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
        _ => return None,
    };
    Some(supported)
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
