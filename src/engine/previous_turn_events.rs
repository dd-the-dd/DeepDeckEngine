//! First-occurrence history of an event in the current turn: `whenever you
//! gain life for the first time each turn` triggers only for the first
//! occurrence of its event this turn, even when the observer was not on the
//! battlefield for an earlier one.
//!
//! The action log is the history. An event kind whose occurrences the log
//! records with their acting player (`lifeGained`, `lifeLost`) maps to that
//! log kind; the occurrence being published is the latest record of that
//! kind for the acting player. A life gain is published as soon as it is
//! recorded, so its occurrence is that one record. Simultaneous life losses
//! (one damage event, CR 120.3) are all recorded before any is published,
//! so their occurrence is the contiguous run of loss records ending at the
//! latest one. An earlier record this turn, outside the occurrence, means
//! the event already happened. A marker record keeps one occurrence from
//! triggering the same rule twice when one publication observes several
//! records.
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::*;

/// The marker a first-occurrence trigger leaves in the action log.
const FIRST_OCCURRENCE_MARKER: &str = "firstOccurrenceTriggered";

/// The event kinds whose occurrences the log records with their acting
/// player under the same kind, and whether one occurrence records a batch
/// of simultaneous records before it is published.
const LOGGED_PLAYER_EVENT_KINDS: [(&str, bool); 2] = [("lifeGained", false), ("lifeLost", true)];

fn logged_player_event_kind(kind: &str) -> Option<bool> {
    LOGGED_PLAYER_EVENT_KINDS
        .iter()
        .find(|(logged, _)| *logged == kind)
        .map(|(_, batched)| *batched)
}

/// `firstTimeEachTurn` is supported on a logged player event of the source
/// controller whose remaining attributes are themselves supported.
pub(super) fn first_occurrence_event_supported(event: &Value) -> bool {
    if event["firstTimeEachTurn"].as_bool() != Some(true) {
        return false;
    }
    let Some(kind) = value_kind(event) else {
        return false;
    };
    let controller = &event["player"];
    if logged_player_event_kind(kind).is_none()
        || value_kind(controller) != Some("controllerOf")
        || value_kind(&controller["object"]) != Some("self")
    {
        return false;
    }
    let mut rest = event.clone();
    if let Some(object) = rest.as_object_mut() {
        object.remove("firstTimeEachTurn");
    }
    triggered_event_supported(&rest)
}

/// A stable identity of one rule of one source for the marker record.
fn rule_identity(rule: &Value) -> String {
    let mut hasher = DefaultHasher::new();
    rule.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

impl GameEngine {
    /// Whether the first-occurrence modifier of `rule` suppresses this
    /// publication: an earlier occurrence of its event happened this turn,
    /// or this rule already triggered for the current one. When it does not
    /// suppress, the marker of this occurrence is recorded.
    pub(super) fn first_occurrence_suppresses(
        &mut self,
        rule: &Value,
        source: &CardInstance,
    ) -> bool {
        let event = &rule["event"];
        if event["firstTimeEachTurn"].as_bool() != Some(true) {
            return false;
        }
        let Some(kind) = value_kind(event).map(str::to_string) else {
            return true;
        };
        // The supported relation names the source controller.
        let player_id = source.controller.clone();
        let turn = self.state.turn_number;
        let records = self
            .state
            .events
            .iter()
            .enumerate()
            .filter(|(_, logged)| {
                logged.turn_number == turn
                    && logged.kind == kind
                    && logged.player_id.as_deref() == Some(player_id.as_str())
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        // Without a record of the occurrence, it cannot be proved first.
        let Some(&latest) = records.last() else {
            return true;
        };
        let batched = logged_player_event_kind(&kind).unwrap_or(false);
        let mut run_start = latest;
        while batched && run_start > 0 {
            let previous = &self.state.events[run_start - 1];
            if previous.turn_number != turn || previous.kind != kind {
                break;
            }
            run_start -= 1;
        }
        if records.iter().any(|index| *index < run_start) {
            return true;
        }
        let identity = rule_identity(rule);
        let already_triggered = self.state.events.iter().any(|logged| {
            logged.turn_number == turn
                && logged.kind == FIRST_OCCURRENCE_MARKER
                && logged.card_instance_id.as_deref() == Some(source.instance_id.as_str())
                && logged.detail["rule"].as_str() == Some(identity.as_str())
        });
        if already_triggered {
            return true;
        }
        self.record_event(
            FIRST_OCCURRENCE_MARKER,
            Some(player_id),
            Some(source.instance_id.clone()),
            json!({ "rule": identity, "eventKind": kind }),
        );
        false
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "previous_turn_events_tests.rs"]
mod tests;
