//! Admission is captured before a movement changes any source zone.
use super::{CardDefinition, GameEngine, Value, matches_card_filter, value_kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BattlefieldEntryAdmission {
    Allowed,
    RemainInSourceZone,
}

/// Only policies active before this movement changes any source zone.
pub(super) struct ZoneEntryPolicySnapshot {
    modifiers: Vec<Value>,
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "zone_entry_tests.rs"]
mod tests;

impl BattlefieldEntryAdmission {
    pub(super) fn is_prohibited(self) -> bool {
        self == Self::RemainInSourceZone
    }
}

impl ZoneEntryPolicySnapshot {
    /// Use the original printed definition, before enterAs/face changes.
    pub(super) fn admission(
        &self,
        card: &CardDefinition,
        source_zone: &str,
    ) -> BattlefieldEntryAdmission {
        if self.modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("prohibitEnterBattlefieldFromZones")
                && modifier["zones"].as_array().is_some_and(|zones| {
                    zones.iter().any(|zone| zone.as_str() == Some(source_zone))
                })
                && matches_card_filter(card, &modifier["where"])
        }) {
            BattlefieldEntryAdmission::RemainInSourceZone
        } else {
            BattlefieldEntryAdmission::Allowed
        }
    }

    /// Separate from admission: this may inspect the chosen entering face.
    pub(super) fn destination_replacement(&self, card: &CardDefinition) -> Option<&'static str> {
        self.modifiers
            .iter()
            .any(|modifier| {
                value_kind(modifier) == Some("prohibitUncastBattlefieldEntry")
                    && matches_card_filter(card, &modifier["where"])
            })
            .then_some("exile")
    }

    pub(super) fn admission_for_id(
        &self,
        engine: &GameEngine,
        instance_id: &str,
        source_zone: &str,
    ) -> BattlefieldEntryAdmission {
        engine
            .state
            .players
            .iter()
            .find_map(|player| {
                let cards = match source_zone {
                    "graveyard" => &player.graveyard,
                    "library" => &player.library,
                    "exile" => &player.exile,
                    "hand" => &player.hand,
                    _ => return None,
                };
                cards
                    .iter()
                    .find(|card| card.instance_id == instance_id)
                    .map(|card| self.admission(&card.definition, source_zone))
            })
            // Absence grants no movement: callers still have to take the card.
            .unwrap_or(BattlefieldEntryAdmission::Allowed)
    }
}

impl GameEngine {
    pub(super) fn capture_zone_entry_policy(&self) -> ZoneEntryPolicySnapshot {
        ZoneEntryPolicySnapshot {
            modifiers: self
                .battlefield_static_modifiers()
                .into_iter()
                .filter_map(|(_, modifier)| {
                    matches!(
                        value_kind(&modifier),
                        Some(
                            "prohibitEnterBattlefieldFromZones" | "prohibitUncastBattlefieldEntry"
                        )
                    )
                    .then_some(modifier)
                })
                .collect(),
        }
    }

    pub(super) fn battlefield_entry_destination_replacement(
        &self,
        card: &CardDefinition,
    ) -> Option<&'static str> {
        self.capture_zone_entry_policy()
            .destination_replacement(card)
    }

    pub(super) fn card_entry_from_zone_replacement(
        &self,
        card: &CardDefinition,
        source_zone: &str,
    ) -> Option<&'static str> {
        let policy = self.capture_zone_entry_policy();
        if policy.admission(card, source_zone).is_prohibited() {
            Some("prohibited")
        } else {
            policy.destination_replacement(card)
        }
    }

    pub(super) fn battlefield_entry_admission(
        &self,
        card: &CardDefinition,
        source_zone: &str,
    ) -> BattlefieldEntryAdmission {
        self.capture_zone_entry_policy()
            .admission(card, source_zone)
    }

    /// Single-object callers capture before removal; batches reuse one snapshot.
    pub(super) fn battlefield_entry_admission_for_id(
        &self,
        instance_id: &str,
        source_zone: &str,
    ) -> BattlefieldEntryAdmission {
        self.capture_zone_entry_policy()
            .admission_for_id(self, instance_id, source_zone)
    }
}
