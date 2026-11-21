use super::*;

impl GameEngine {
    /// Admit ordinary battlefield observers while traversing the battlefield or
    /// capturing a departure snapshot. A departed source snapshot is allowed;
    /// live battlefield lookup would discard legitimate look-back observers.
    /// Do not apply this admission check again to frozen observers or queued
    /// triggers, or use it for an ability observing its own phasing event.
    pub(super) fn battlefield_observer_rule_active(
        &self,
        source: &CardInstance,
        rule: &Value,
    ) -> bool {
        !source.flags.get("phasedOut").copied().unwrap_or(false)
            && self.rule_active_for_source(source, rule)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "trigger_observer_presence_tests.rs"]
mod tests;
