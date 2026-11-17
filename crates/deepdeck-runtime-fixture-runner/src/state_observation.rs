//! Read-only literal field matching for fixture observations, never simulation.
use deepdeck_engine_runtime::engine::GameState;
use serde_json::Value;
use std::collections::BTreeMap;

fn matches_fields(value: &Value, fields: &BTreeMap<String, Value>) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    fields
        .iter()
        .all(|(key, expected)| object.get(key).is_some_and(|actual| actual == expected))
}

pub(super) fn event_details(
    state: &GameState,
    event_kind: &str,
    player_id: Option<&str>,
    fields: &BTreeMap<String, Value>,
    equals: usize,
) -> (bool, String) {
    let actual = state
        .events
        .iter()
        .filter(|event| {
            event.kind == event_kind
                && player_id.is_none_or(|player| event.player_id.as_deref() == Some(player))
                && matches_fields(&event.detail, fields)
        })
        .count();
    (
        actual == equals,
        format!(
            "event {event_kind} actor {player_id:?} exact fields {fields:?} expected count {equals}, actual {actual}"
        ),
    )
}

pub(super) fn rule_modifier_count(
    state: &GameState,
    fields: &BTreeMap<String, Value>,
    equals: usize,
) -> (bool, String) {
    let actual = state
        .rule_modifiers
        .iter()
        .filter(|modifier| matches_fields(modifier, fields))
        .count();
    (
        actual == equals,
        format!("rule modifiers exact fields {fields:?} expected count {equals}, actual {actual}"),
    )
}

#[cfg(test)]
#[path = "state_observation_tests.rs"]
mod tests;
