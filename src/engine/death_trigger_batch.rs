//! Stable trigger sources for a batch of state-based deaths.
//!
//! State-based actions first determine the complete lethal set. The engine must
//! therefore remember every active death-trigger source before removing the
//! first permanent; otherwise a source removed early can miss a later death in
//! the same batch.

use serde_json::Value;

use super::{
    CardInstance, GameEngine, triggered_event_variants, triggered_rule_supported, value_kind,
};

pub(super) struct DeathTriggerSource {
    pub(super) source: CardInstance,
    pub(super) rules: Vec<Value>,
}

pub(super) struct DeathTriggerSourceSnapshot {
    sources: Vec<DeathTriggerSource>,
}

impl DeathTriggerSourceSnapshot {
    pub(super) fn sources(&self) -> &[DeathTriggerSource] {
        &self.sources
    }
}

fn rule_observes_permanent_death(rule: &Value) -> bool {
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
        let sources = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .filter_map(|source| {
                let rules = source
                    .definition
                    .rules
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_active_for_source(source, rule)
                            && rule_observes_permanent_death(rule)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                (!rules.is_empty()).then(|| DeathTriggerSource {
                    source: source.clone(),
                    rules,
                })
            })
            .collect();
        DeathTriggerSourceSnapshot { sources }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "death_trigger_batch_tests.rs"]
mod tests;
