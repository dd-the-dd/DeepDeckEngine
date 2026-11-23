//! Suspend (CR 702.62) as a typed keyword expansion.
//!
//! The printed keyword (or a suspend granted by an effect) keeps its
//! hand-zone special action in the casting module. Its two exile-zone
//! triggered abilities are expanded here into ordinary triggered rules made
//! of shared primitives: a `stepBegan` upkeep event guarded by the
//! `sourceIsSuspended` intervening-if condition that removes one time counter,
//! and a last-counter `counterRemoved` event guarded by `sourceInZone` that
//! casts the card without paying its mana cost through `castAnyNumber`.
use super::object_identity::existing_object_identity_of;
use super::*;

/// CR 702.62b requires the card to "have suspend": printed, or granted by an
/// effect that exiled it with time counters (the `suspended` designation).
pub(super) fn card_has_suspend(card: &CardInstance) -> bool {
    card.flags.get("suspended").copied().unwrap_or(false)
        || card.definition.printed_rules().iter().any(|rule| {
            value_kind(rule) == Some("keywordAbility")
                && suspend_ability_supported(&rule["ability"])
        })
}

/// Keyword-expanded triggered rules an object has in addition to its printed
/// rules. They are ordinary triggered abilities with an explicit trigger zone.
pub(super) fn keyword_triggered_rules(card: &CardInstance) -> Vec<Value> {
    if !card_has_suspend(card) {
        return Vec::new();
    }
    vec![suspend_upkeep_rule(), suspend_last_counter_rule()]
}

/// "At the beginning of your upkeep, if this card is suspended, remove a time
/// counter from it."
fn suspend_upkeep_rule() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "triggerZone": "exile",
        "event": {
            "kind": "stepBegan",
            "step": "upkeep",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        },
        "condition": { "kind": "sourceIsSuspended" },
        "effects": [{
            "kind": "removeCounters",
            "permanent": { "kind": "self" },
            "counter": "time",
            "count": { "kind": "integer", "value": 1 },
        }],
    })
}

/// "When the last time counter is removed from this card, if it's exiled, you
/// may play it without paying its mana cost if able. ... If you cast a creature
/// spell this way, it gains haste until you lose control of the spell or the
/// permanent it becomes." `castVia: suspend` carries the CR 702.62d casting
/// provenance that the shared casting path turns into that haste.
fn suspend_last_counter_rule() -> Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "triggerZone": "exile",
        "event": {
            "kind": "counterRemoved",
            "object": { "kind": "self" },
            "counter": "time",
            "last": true,
        },
        "condition": { "kind": "sourceInZone", "zone": "exile" },
        "effects": [{
            "kind": "castAnyNumber",
            "player": { "kind": "abilityController" },
            "cards": { "kind": "self" },
            "where": { "kind": "canBeCastAsSpell" },
            "timing": { "kind": "duringResolution" },
            "withoutPayingManaCost": true,
            "alternativeCostsAllowed": false,
            "additionalCostsApply": true,
            "variableManaValue": { "kind": "integer", "value": 0 },
            "sourceZone": "exile",
            "maximum": { "kind": "integer", "value": 1 },
            "castVia": "suspend",
        }],
    })
}

/// A self counter-removal event pattern: `{counterRemoved, object: self,
/// counter, last}`. `last: true` matches only the removal that leaves none.
pub(super) fn counter_removed_event_supported(event: &Value) -> bool {
    value_kind(&event["object"]) == Some("self")
        && event["counter"].is_string()
        && event.get("last").is_none_or(Value::is_boolean)
}

/// Supported casting provenance for `castAnyNumber`.
pub(super) fn cast_via_supported(value: &Value) -> bool {
    value.as_str() == Some("suspend")
}

/// The decision flag the shared casting path reads for a given provenance.
pub(super) fn cast_via_decision(value: &Value) -> Option<&'static str> {
    match value.as_str() {
        Some("suspend") => Some("castFromSuspend"),
        _ => None,
    }
}

/// The zone a trigger rule functions from, defaulting to the battlefield.
pub(super) fn rule_trigger_zone(rule: &Value) -> &str {
    rule["triggerZone"].as_str().unwrap_or("battlefield")
}

impl GameEngine {
    /// The same object as `source`, still in exile. A stale incarnation is a
    /// new object (CR 400.7) and does not match.
    pub(super) fn exiled_object_position(&self, source: &CardInstance) -> Option<(usize, usize)> {
        let source_identity = existing_object_identity_of(source).ok().flatten();
        self.state
            .players
            .iter()
            .enumerate()
            .find_map(|(player_index, player)| {
                player
                    .exile
                    .iter()
                    .position(|card| {
                        card.instance_id == source.instance_id
                            && source_identity.as_ref().is_none_or(|identity| {
                                existing_object_identity_of(card)
                                    .ok()
                                    .flatten()
                                    .is_none_or(|current| &current == identity)
                            })
                    })
                    .map(|card_index| (player_index, card_index))
            })
    }

    /// CR 702.62b: in exile, has suspend, and has a time counter.
    pub(super) fn source_is_suspended(&self, source: &CardInstance) -> bool {
        self.exiled_object_position(source)
            .is_some_and(|(player_index, card_index)| {
                let card = &self.state.players[player_index].exile[card_index];
                card_has_suspend(card) && card.counters.get("time").copied().unwrap_or(0) > 0
            })
    }

    /// The ability source is currently in the named zone.
    pub(super) fn source_in_zone(&self, source: &CardInstance, zone: &str) -> bool {
        match zone {
            "exile" => self.exiled_object_position(source).is_some(),
            "battlefield" => self.permanent_position(&source.instance_id).is_some(),
            _ => false,
        }
    }

    /// Trigger the source's own "when the last [counter] is removed" rules,
    /// printed or keyword-expanded, that function from the source's zone.
    pub(super) fn enqueue_counter_removed_triggers(
        &mut self,
        source: &CardInstance,
        source_zone: &str,
        counter: &str,
        remaining: i32,
    ) {
        let rules = source
            .definition
            .printed_rules()
            .iter()
            .cloned()
            .chain(keyword_triggered_rules(source))
            .filter(|rule| {
                triggered_rule_supported(rule)
                    && rule_trigger_zone(rule) == source_zone
                    && triggered_event_variants(&rule["event"])
                        .iter()
                        .any(|event| {
                            value_kind(event) == Some("counterRemoved")
                                && value_kind(&event["object"]) == Some("self")
                                && event["counter"].as_str() == Some(counter)
                                && (event["last"].as_bool() != Some(true) || remaining == 0)
                        })
                    && rule.get("condition").is_none_or(|condition| {
                        self.condition_value_with_source(
                            condition,
                            &source.controller,
                            &BTreeMap::new(),
                            Some(source),
                        )
                    })
            })
            .collect::<Vec<_>>();
        for rule in rules {
            self.put_triggered_ability_on_stack(source, rule);
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}
