//! Read-only setup assertion. It never assigns or repairs engine identities.
use deepdeck_engine_runtime::engine::GameState;
use std::collections::BTreeSet;

pub(super) fn evaluate_setup_object_identities(state: &GameState, equals: usize) -> (bool, String) {
    if !state.stack.is_empty() {
        return (
            false,
            "setup identity assertion requires an empty stack".into(),
        );
    }
    let mut physical = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for player in &state.players {
        for card in [
            &player.library,
            &player.hand,
            &player.battlefield,
            &player.graveyard,
            &player.exile,
            &player.command_zone,
        ]
        .into_iter()
        .flat_map(|zone| zone.iter())
        {
            if card.instance_id.is_empty() || !physical.insert(card.instance_id.as_str()) {
                return (
                    false,
                    format!("duplicate/empty setup carrier {}", card.instance_id),
                );
            }
            let active = card
                .flags
                .iter()
                .filter(|(key, value)| **value && key.starts_with("trackedZoneObject:"))
                .map(|(key, _)| &key["trackedZoneObject:".len()..])
                .collect::<Vec<_>>();
            if active.len() != 1 || active[0].is_empty() || !identities.insert(active[0]) {
                return (
                    false,
                    format!(
                        "missing/duplicate setup incarnation for {}",
                        card.instance_id
                    ),
                );
            }
            let Some(encoded) = active[0]
                .strip_prefix("object:incarnation:")
                .or_else(|| active[0].strip_prefix("battlefield:incarnation:"))
            else {
                return (
                    false,
                    format!(
                        "setup incarnation lacks serialized allocator provenance: {}",
                        card.instance_id
                    ),
                );
            };
            let Some((serial, carrier)) = encoded.split_once(':') else {
                return (false, "invalid setup incarnation encoding".into());
            };
            let Ok(serial) = serial.parse::<usize>() else {
                return (false, "invalid setup incarnation serial".into());
            };
            if serial == 0
                || serial > state.battlefield_incarnation_sequence
                || carrier != card.instance_id
            {
                return (
                    false,
                    format!(
                        "setup incarnation/allocator mismatch for {}",
                        card.instance_id
                    ),
                );
            }
        }
    }
    (
        physical.len() == equals,
        format!(
            "setup admitted carriers expected {equals}, observed {} unique identities {}",
            physical.len(),
            identities.len()
        ),
    )
}
#[cfg(test)]
#[path = "setup_identity_tests.rs"]
mod tests;
