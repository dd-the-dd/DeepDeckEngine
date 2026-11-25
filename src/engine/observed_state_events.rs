//! State events of an observed permanent (docs/oracle-event-model.md).
//!
//! A permanent's own `becomes tapped` / `becomes untapped` abilities keep
//! their established publication. This module publishes the same transitions
//! to every battlefield observer whose pattern names another permanent by
//! the shared object query:
//!
//! ```json
//! {"kind": "permanentTapped" | "permanentUntapped",
//!  "object": {"kind": "permanents", "controller": PLAYER, "where": FILTER,
//!             "excludeSource": true}}
//! ```
//!
//! and the completion signal of a keyword action performed by such a
//! permanent (`explores`, `connives`, CR 701.44, 701.50):
//!
//! ```json
//! {"kind": "abilityPerformed", "abilityId": ID, "player": PLAYER,
//!  "object": {"kind": "permanents", ...}}
//! ```
//!
//! A tapped transition happens only when an untapped permanent becomes
//! tapped (CR 701.26a): a permanent that enters the battlefield tapped never
//! became tapped, so the entry paths publish nothing here. Each observer
//! binds the observed permanent as the triggering permanent.
use super::*;

const OBJECT_FIELDS: [&str; 4] = ["kind", "controller", "where", "excludeSource"];

fn only_fields(value: &Value, fields: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
}

/// The `permanents` object of an observed-permanent pattern.
fn observed_object_supported(object: &Value) -> bool {
    value_kind(object) == Some("permanents")
        && only_fields(object, &OBJECT_FIELDS)
        && object.get("where").is_none_or(card_filter_supported)
        && object
            .get("controller")
            .is_none_or(player_action_events::event_player_reference_supported)
        && object.get("excludeSource").is_none_or(Value::is_boolean)
}

/// Support of an observed-permanent state pattern: `Some` for a tap
/// transition or a keyword-action signal that names an observed permanent,
/// `None` for every other event (including the source's own forms).
pub(super) fn observed_state_event_supported(event: &Value) -> Option<bool> {
    match value_kind(event) {
        Some("permanentTapped" | "permanentUntapped")
            if value_kind(&event["object"]) == Some("permanents") =>
        {
            Some(
                only_fields(event, &["kind", "object"])
                    && observed_object_supported(&event["object"]),
            )
        }
        Some("abilityPerformed") if event.get("object").is_some() => Some(
            only_fields(event, &["kind", "abilityId", "player", "object"])
                && event["abilityId"]
                    .as_str()
                    .is_some_and(|ability_id| !ability_id.trim().is_empty())
                && player_action_events::event_player_reference_supported(&event["player"])
                && observed_object_supported(&event["object"]),
        ),
        _ => None,
    }
}

/// Whether an observer pattern carries an observed permanent: the historical
/// publication of its kind (which ignores this field) must skip it.
pub(super) fn names_observed_permanent(event: &Value) -> bool {
    value_kind(&event["object"]) == Some("permanents")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ObservedState<'a> {
    Tapped,
    Untapped,
    Performed {
        ability_id: &'a str,
        player_id: &'a str,
    },
}

impl GameEngine {
    /// The `permanents` object of `event` observed by `source` holds for
    /// `observed`: its controller relation, its effective characteristics
    /// and the source exclusion.
    fn observed_permanent_matches(
        &self,
        object: &Value,
        source: &CardInstance,
        observed: &CardInstance,
    ) -> bool {
        value_kind(object) == Some("permanents")
            && !(object["excludeSource"].as_bool() == Some(true)
                && source.instance_id == observed.instance_id)
            && object.get("controller").is_none_or(|controller| {
                self.event_player_reference_matches(controller, source, &observed.controller)
            })
            && object
                .get("where")
                .is_none_or(|filter| self.permanent_matches_effective_filter(observed, filter))
    }

    fn observed_state_matches(
        &self,
        event: &Value,
        state: ObservedState<'_>,
        source: &CardInstance,
        observed: &CardInstance,
    ) -> bool {
        let kind_matches = match state {
            ObservedState::Tapped => value_kind(event) == Some("permanentTapped"),
            ObservedState::Untapped => value_kind(event) == Some("permanentUntapped"),
            ObservedState::Performed {
                ability_id,
                player_id,
            } => {
                value_kind(event) == Some("abilityPerformed")
                    && event["abilityId"].as_str() == Some(ability_id)
                    && self.event_player_reference_matches(&event["player"], source, player_id)
            }
        };
        kind_matches && self.observed_permanent_matches(&event["object"], source, observed)
    }

    /// Every battlefield observer of `state` for `observed`, in battlefield
    /// order, each bound to the observed permanent.
    fn publish_observed_state(&mut self, state: ObservedState<'_>, observed: &CardInstance) {
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
                                    self.observed_state_matches(event, state, source, observed)
                                })
                    })
                    .map(|rule| (source.clone(), rule.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut decisions = BTreeMap::from([
            (
                "triggeringPermanentId".to_string(),
                Value::String(observed.instance_id.clone()),
            ),
            (
                "triggeringPermanentController".to_string(),
                Value::String(observed.controller.clone()),
            ),
        ]);
        if let ObservedState::Performed { player_id, .. } = state {
            decisions.insert(
                "triggeringPlayerId".to_string(),
                Value::String(player_id.to_string()),
            );
        }
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    fn battlefield_permanent(&self, instance_id: &str) -> Option<CardInstance> {
        self.permanent_position(instance_id)
            .map(|(player_index, card_index)| {
                self.state.players[player_index].battlefield[card_index].clone()
            })
    }

    /// An untapped permanent became tapped (CR 701.26a).
    pub(super) fn publish_observed_permanent_tapped(&mut self, tapped: &CardInstance) {
        self.publish_observed_state(ObservedState::Tapped, tapped);
    }

    /// A tapped permanent became untapped (CR 701.26b).
    pub(super) fn publish_observed_permanent_untapped(&mut self, instance_id: &str) {
        let Some(untapped) = self.battlefield_permanent(instance_id) else {
            return;
        };
        self.publish_observed_state(ObservedState::Untapped, &untapped);
    }

    /// A keyword action of `object_id` completed and signalled `ability_id`
    /// for `player_id` (`ability_signals.rs`). The acting permanent is read
    /// on the battlefield; an action whose permanent already left it has no
    /// observed permanent to match.
    pub(super) fn publish_observed_ability_signal(
        &mut self,
        player_id: &str,
        ability_id: &str,
        object_id: Option<&str>,
    ) {
        let Some(observed) =
            object_id.and_then(|instance_id| self.battlefield_permanent(instance_id))
        else {
            return;
        };
        self.publish_observed_state(
            ObservedState::Performed {
                ability_id,
                player_id,
            },
            &observed,
        );
    }
}
