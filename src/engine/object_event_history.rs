//! Per-object zone history of a card outside the battlefield, read by the
//! shared object query (`cards in your graveyard that were put there from
//! the battlefield this turn`, CR 400.7, 603.10a).
//!
//! A card that changes zones is a new object, but the engine keeps the card
//! instance's identifier across the move and records every zone arrival as
//! an event of the current turn. The history atom is therefore a fact about
//! the card instance, not about its definition: the definition-only filter
//! matcher cannot answer it. Consumers that know the candidate's instance
//! bind the atom first (`bind_card_history_leaves`), replacing each history
//! leaf by the constant predicate it evaluates to, and then match the rest
//! of the filter as before.
//!
//! The arrival events read here are the ones the engine records when a card
//! enters a graveyard or exile: the departure of a permanent (CR 700.4),
//! milling, discarding and the batched graveyard arrival. The card was put
//! into its zone from the battlefield this turn when its latest arrival of
//! this turn is a battlefield departure.

use std::borrow::Cow;

use serde_json::{Value, json};

use super::{GameEngine, GameEvent, value_kind};

/// The history atom of a card in a zone.
pub(super) const PUT_THERE_FROM_BATTLEFIELD: &str = "putThereFromBattlefieldThisTurn";

/// Event kinds recording a permanent leaving the battlefield for the zone
/// the card is now in.
const BATTLEFIELD_DEPARTURES: &[&str] = &[
    "permanentToGraveyard",
    "permanentToExile",
    "permanentExiled",
    "permanentLeftBattlefield",
];

/// Event kinds recording a card arriving in a graveyard from elsewhere.
const OTHER_ARRIVALS: &[&str] = &["cardMilled", "cardDiscarded"];

/// Whether `kind` is a card zone-history atom of this module.
pub(super) fn is_card_history_kind(kind: &str) -> bool {
    kind == PUT_THERE_FROM_BATTLEFIELD
}

/// Whether a history atom occurs anywhere in `filter`.
fn filter_has_card_history(filter: &Value) -> bool {
    match value_kind(filter) {
        Some(kind) if is_card_history_kind(kind) => true,
        Some("and" | "or") => filter["operands"]
            .as_array()
            .is_some_and(|operands| operands.iter().any(filter_has_card_history)),
        Some("not") => filter_has_card_history(&filter["operand"]),
        _ => false,
    }
}

/// The constant predicate a bound history leaf becomes: an empty
/// conjunction holds, an empty disjunction does not.
fn constant(value: bool) -> Value {
    json!({ "kind": if value { "and" } else { "or" }, "operands": [] })
}

/// Whether `event` records an arrival of `instance_id` in a zone.
fn names_arrival(event: &GameEvent, instance_id: &str) -> bool {
    if event.kind == "cardsEnteredGraveyard" {
        return event.detail["cardIds"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(instance_id)));
    }
    (BATTLEFIELD_DEPARTURES.contains(&event.kind.as_str())
        || OTHER_ARRIVALS.contains(&event.kind.as_str()))
        && event.card_instance_id.as_deref() == Some(instance_id)
}

impl GameEngine {
    /// CR 400.7, 603.10a: the card's latest zone arrival of this turn was
    /// its departure from the battlefield. A card that arrived otherwise
    /// afterwards (discarded after returning to its owner's hand, milled
    /// after being put on top of its library) is not one.
    pub(super) fn card_put_there_from_battlefield_this_turn(&self, instance_id: &str) -> bool {
        self.state
            .events
            .iter()
            .rev()
            .take_while(|event| event.turn_number == self.state.turn_number)
            .find(|event| names_arrival(event, instance_id))
            .is_some_and(|event| BATTLEFIELD_DEPARTURES.contains(&event.kind.as_str()))
    }

    /// `filter` with every card history leaf replaced by its value for
    /// `instance_id`. A filter without one is returned unchanged.
    pub(super) fn bind_card_history_leaves<'a>(
        &self,
        instance_id: &str,
        filter: &'a Value,
    ) -> Cow<'a, Value> {
        if !filter_has_card_history(filter) {
            return Cow::Borrowed(filter);
        }
        Cow::Owned(self.bind_card_history_value(instance_id, filter))
    }

    fn bind_card_history_value(&self, instance_id: &str, filter: &Value) -> Value {
        match value_kind(filter) {
            Some(PUT_THERE_FROM_BATTLEFIELD) => {
                constant(self.card_put_there_from_battlefield_this_turn(instance_id))
            }
            Some("and" | "or") => {
                let mut bound = filter.clone();
                if let Some(operands) = filter["operands"].as_array() {
                    bound["operands"] = Value::Array(
                        operands
                            .iter()
                            .map(|operand| self.bind_card_history_value(instance_id, operand))
                            .collect(),
                    );
                }
                bound
            }
            Some("not") => {
                let mut bound = filter.clone();
                bound["operand"] = self.bind_card_history_value(instance_id, &filter["operand"]);
                bound
            }
            _ => filter.clone(),
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "object_event_history_tests.rs"]
mod tests;
