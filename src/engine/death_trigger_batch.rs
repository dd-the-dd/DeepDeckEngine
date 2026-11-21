//! Stable trigger sources for a batch of state-based deaths.
//!
//! State-based actions first determine the complete lethal set. The engine must
//! therefore remember every active death-trigger source before removing the
//! first permanent; otherwise a source removed early can miss a later death in
//! the same batch.

use serde_json::Value;

use super::{GameEngine, triggered_event_variants, value_kind};

pub(super) use super::trigger_multiplicity::CapturedTriggerSource as DeathTriggerSource;

pub(super) struct DeathTriggerSourceSnapshot {
    sources: Vec<DeathTriggerSource>,
}

impl DeathTriggerSourceSnapshot {
    pub(super) fn sources(&self) -> &[DeathTriggerSource] {
        &self.sources
    }
}

pub(super) fn rule_observes_permanent_death(rule: &Value) -> bool {
    triggered_event_variants(&rule["event"])
        .iter()
        .any(|event| {
            matches!(
                value_kind(event),
                Some(
                    "permanentDied"
                        | "attachedPermanentDied"
                        | "smallControlledCreatureDied"
                        | "opponentCreatureDied"
                        | "controlledCreatureDiedOrExiled"
                        | "sourceDiedOrExiled"
                )
            )
        })
}

impl GameEngine {
    pub(super) fn capture_death_trigger_source_snapshot(&self) -> DeathTriggerSourceSnapshot {
        Self::death_trigger_snapshot_from_sources(
            self.capture_battlefield_departure_trigger_sources(),
        )
    }

    pub(super) fn death_trigger_snapshot_from_sources(
        sources: Vec<DeathTriggerSource>,
    ) -> DeathTriggerSourceSnapshot {
        DeathTriggerSourceSnapshot { sources }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "death_trigger_batch_tests.rs"]
mod tests;
