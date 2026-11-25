//! The end of combat step (CR 511).
//!
//! At its beginning, the battlefield's `at end of combat` triggered
//! abilities (`stepBegan` with step `endOfCombat`, CR 511.2) trigger together
//! with the delayed triggered abilities waiting for it (CR 603.7). The step
//! happens in every combat phase, also when no creature attacks: declaring no
//! attackers skips only the declare blockers and combat damage steps (CR
//! 508.8).
use super::*;

impl GameEngine {
    /// Puts the end-of-combat triggered abilities on the stack; returns
    /// whether any was put there, so that the caller opens the step's
    /// priority window.
    pub(super) fn begin_end_of_combat_step(&mut self) -> usize {
        let before = self.state.stack.len();
        self.enqueue_step_triggers("endOfCombat");
        let delayed = self.fire_delayed_step_triggers("endOfCombat");
        delayed + self.state.stack.len().saturating_sub(before)
    }

    /// A combat phase in which no creature attacked still has its end of
    /// combat step (CR 508.8, 511): the same triggers, priority window and
    /// end-of-combat cleanup as a combat with attackers.
    pub(super) fn end_combat_without_attackers<P: DecisionProvider>(
        &mut self,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        if self.begin_end_of_combat_step() > 0 {
            self.run_priority_window(provider)?;
            if self.state.outcome.is_some() || self.force_end_turn {
                self.clear_combat_state();
                return Ok(());
            }
        }
        self.process_end_combat_exiles()?;
        self.drain_linked_exile_returns(provider)?;
        self.clear_combat_state();
        Ok(())
    }
}
