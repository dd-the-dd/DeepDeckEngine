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
/// `that was put there this turn`: the card arrived in its zone this turn.
pub(super) const PUT_THERE_THIS_TURN: &str = "putThereThisTurn";
/// `that was milled this turn` (CR 701.13b): its latest arrival was a mill.
pub(super) const MILLED_THIS_TURN: &str = "milledThisTurn";

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

/// The per-card record of a mill (CR 701.13).
const MILL_ARRIVAL: &str = "cardMilled";

/// Whether `kind` is a card zone-history atom of this module.
pub(super) fn is_card_history_kind(kind: &str) -> bool {
    matches!(
        kind,
        PUT_THERE_FROM_BATTLEFIELD | PUT_THERE_THIS_TURN | MILLED_THIS_TURN
    )
}

/// How a card arrived in the zone it is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ZoneArrival {
    /// A departure of the permanent from the battlefield (CR 700.4).
    FromBattlefield,
    /// A mill: put into the graveyard from its owner's library (CR 701.13).
    Milled,
    /// Any other recorded arrival (a discard).
    Other,
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

/// How `event` records an arrival of `instance_id` in a zone, if it does:
/// `Some(true)` for a departure from the battlefield, `Some(false)` for any
/// other arrival. The batched graveyard arrival is recorded after the
/// per-card departure, mill or discard record of the same move, so it is
/// classified by its recorded origin zone; an arrival whose origin the
/// recorder did not know is never read as a battlefield departure.
/// The batched record names the library as the origin of the cards a mill
/// puts into a graveyard (the per-card `cardMilled` record precedes it).
fn zone_arrival(event: &GameEvent, instance_id: &str) -> Option<ZoneArrival> {
    if event.kind == "cardsEnteredGraveyard" {
        let names_card = event.detail["cardIds"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(instance_id)));
        return names_card.then(|| match event.detail["from"].as_str() {
            Some("battlefield") => ZoneArrival::FromBattlefield,
            Some("library") => ZoneArrival::Milled,
            _ => ZoneArrival::Other,
        });
    }
    if event.card_instance_id.as_deref() != Some(instance_id) {
        return None;
    }
    if BATTLEFIELD_DEPARTURES.contains(&event.kind.as_str()) {
        Some(ZoneArrival::FromBattlefield)
    } else if event.kind == MILL_ARRIVAL {
        Some(ZoneArrival::Milled)
    } else if OTHER_ARRIVALS.contains(&event.kind.as_str()) {
        Some(ZoneArrival::Other)
    } else {
        None
    }
}

impl GameEngine {
    /// CR 400.7, 603.10a: the card's latest zone arrival of this turn was
    /// its departure from the battlefield. A card that arrived otherwise
    /// afterwards (discarded after returning to its owner's hand, milled
    /// after being put on top of its library) is not one.
    pub(super) fn card_put_there_from_battlefield_this_turn(&self, instance_id: &str) -> bool {
        self.latest_zone_arrival_this_turn(instance_id) == Some(ZoneArrival::FromBattlefield)
    }

    /// The card's latest recorded zone arrival of this turn, if any.
    fn latest_zone_arrival_this_turn(&self, instance_id: &str) -> Option<ZoneArrival> {
        self.state
            .events
            .iter()
            .rev()
            .take_while(|event| event.turn_number == self.state.turn_number)
            .find_map(|event| zone_arrival(event, instance_id))
    }

    /// The value of one card zone-history atom for this card instance:
    /// `putThereFromBattlefieldThisTurn`, `putThereThisTurn` (any recorded
    /// arrival this turn) or `milledThisTurn` (CR 701.13b). `None` for any
    /// other kind.
    pub(super) fn card_history_matches(&self, instance_id: &str, kind: &str) -> Option<bool> {
        Some(match kind {
            PUT_THERE_FROM_BATTLEFIELD => {
                self.card_put_there_from_battlefield_this_turn(instance_id)
            }
            PUT_THERE_THIS_TURN => self.latest_zone_arrival_this_turn(instance_id).is_some(),
            MILLED_THIS_TURN => {
                self.latest_zone_arrival_this_turn(instance_id) == Some(ZoneArrival::Milled)
            }
            _ => return None,
        })
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
            Some(kind) if is_card_history_kind(kind) => constant(
                self.card_history_matches(instance_id, kind)
                    .unwrap_or(false),
            ),
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
