//! Simultaneous-event identity for `one or more` triggers (CR 603.2c).
//!
//! One instruction of a resolving effect, or one state-based-action pass
//! (CR 704.3), is one event, however many objects it moves. A trigger whose
//! event is aggregated (`"aggregation": "oneOrMore"`) triggers at most once per
//! source, rule and event; a per-object trigger is unaffected. A zone change
//! outside any scope is its own event, which never merges unrelated changes.
//! Instructions repeated for each player (`Each player sacrifices a creature`)
//! happen simultaneously (CR 101.4): the k-th child instruction of one player
//! iteration shares one event across all players.
use std::collections::BTreeSet;

use serde_json::Value;

use super::GameEngine;

#[derive(Clone, Debug, Default)]
pub(super) struct EventBatchState {
    current: Option<u64>,
    next: u64,
    depth: usize,
    shared: Option<SharedChildBatches>,
    fired: BTreeSet<(String, String, u64)>,
    /// Discards of an open event, published as one `one or more` discard
    /// per player when that event ends (CR 603.2c).
    pending_discards: Vec<(u64, String, String)>,
}

/// One event per child position of a player iteration, reused by every player.
#[derive(Clone, Debug)]
pub(super) struct SharedChildBatches {
    depth: usize,
    ids: Vec<u64>,
    position: usize,
}

/// The enclosing scope to restore when the opened one ends.
#[must_use]
pub(super) struct EventBatchScope {
    previous: Option<u64>,
    shared: bool,
}

pub(super) fn event_aggregation_supported(event: &Value) -> bool {
    event
        .get("aggregation")
        .is_none_or(|value| matches!(value.as_str(), Some("perObject" | "oneOrMore")))
}

pub(super) fn event_is_aggregated(event: &Value) -> bool {
    event["aggregation"].as_str() == Some("oneOrMore")
}

impl EventBatchState {
    /// The simultaneous event currently open, if any.
    pub(super) fn current_batch(&self) -> Option<u64> {
        self.current
    }

    /// Defers one discarded card of `player_id` to the end of `batch`.
    pub(super) fn defer_discard(&mut self, batch: u64, player_id: &str, card_id: &str) {
        self.pending_discards
            .push((batch, player_id.to_string(), card_id.to_string()));
    }

    /// The deferred discards of `batch`, in discard order.
    pub(super) fn take_discards(&mut self, batch: u64) -> Vec<(String, String)> {
        let (taken, kept) = std::mem::take(&mut self.pending_discards)
            .into_iter()
            .partition::<Vec<_>, _>(|(pending, _, _)| *pending == batch);
        self.pending_discards = kept;
        taken
            .into_iter()
            .map(|(_, player_id, card_id)| (player_id, card_id))
            .collect()
    }

    fn allocate(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

impl GameEngine {
    /// Opens a new event for one instruction or one state-based-action pass,
    /// or joins the shared event of a player iteration's child position.
    pub(super) fn begin_event_batch(&mut self) -> EventBatchScope {
        let previous = self.event_batches.current;
        self.event_batches.depth += 1;
        let depth = self.event_batches.depth;
        let shared = self
            .event_batches
            .shared
            .as_mut()
            .filter(|plan| plan.depth == depth && plan.position < plan.ids.len())
            .map(|plan| {
                plan.position += 1;
                plan.ids[plan.position - 1]
            });
        let id = match shared {
            Some(id) => id,
            None => self.event_batches.allocate(),
        };
        self.event_batches.current = Some(id);
        EventBatchScope {
            previous,
            shared: shared.is_some(),
        }
    }

    pub(super) fn end_event_batch(&mut self, scope: EventBatchScope) {
        let ended = self.event_batches.current;
        self.event_batches.current = scope.previous;
        self.event_batches.depth -= 1;
        if let Some(ended) = ended {
            self.flush_discard_batch(ended);
        }
        // A shared event stays open for the next player; its claims are
        // released when the whole iteration ends.
        if !scope.shared
            && let Some(ended) = ended
        {
            self.event_batches
                .fired
                .retain(|(_, _, batch)| *batch != ended);
        }
    }

    /// Plans one shared event per child instruction of a player iteration.
    /// Child instructions run one level below the iteration instruction.
    pub(super) fn begin_shared_child_batches(
        &mut self,
        children: usize,
    ) -> Option<SharedChildBatches> {
        let ids = (0..children)
            .map(|_| self.event_batches.allocate())
            .collect();
        self.event_batches.shared.replace(SharedChildBatches {
            depth: self.event_batches.depth + 1,
            ids,
            position: 0,
        })
    }

    /// Each player's children start again at the first shared event.
    pub(super) fn restart_shared_child_batches(&mut self) {
        if let Some(plan) = self.event_batches.shared.as_mut() {
            plan.position = 0;
        }
    }

    pub(super) fn end_shared_child_batches(&mut self, previous: Option<SharedChildBatches>) {
        if let Some(plan) = std::mem::replace(&mut self.event_batches.shared, previous) {
            self.event_batches
                .fired
                .retain(|(_, _, batch)| !plan.ids.contains(batch));
        }
    }

    /// For a rule observing an aggregated event, true when it already
    /// triggered for this source in the current event (and records it otherwise).
    pub(super) fn aggregated_trigger_already_claimed(
        &mut self,
        source_id: &str,
        rule: &Value,
    ) -> bool {
        super::triggered_event_variants(&rule["event"])
            .into_iter()
            .any(event_is_aggregated)
            && !self.claim_aggregated_trigger(source_id, rule)
    }

    /// True when this aggregated trigger has not yet triggered for the
    /// current event; records it. Outside a scope every change is its own event.
    pub(super) fn claim_aggregated_trigger(&mut self, source_id: &str, rule: &Value) -> bool {
        let Some(batch) = self.event_batches.current else {
            return true;
        };
        self.event_batches
            .fired
            .insert((source_id.to_string(), rule.to_string(), batch))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4")
))]
#[path = "event_batch_tests.rs"]
mod tests;
