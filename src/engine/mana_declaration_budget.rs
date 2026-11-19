//! Mana declaration budgets reuse the historical sum of per-source maxima.
//! The checked callback changes error transport, never the quantity heuristic.
use super::mana_checked_discovery::CheckedManaActivationPlan;
use super::mana_projection_context::ManaProjectionContext;
use super::*;

impl GameEngine {
    pub(super) fn maximum_potential_mana(
        &self,
        controller_id: &str,
        spending_card: Option<&CardDefinition>,
    ) -> usize {
        match self.try_maximum_potential_mana_with(
            controller_id,
            spending_card,
            &mut |source, spending| {
                Ok::<_, std::convert::Infallible>(
                    self.mana_choices_for_permanent(source, spending, false, false),
                )
            },
        ) {
            Ok(maximum) => maximum,
            Err(never) => match never {},
        }
    }

    pub(super) fn checked_mana_budget_in_context(
        &self,
        controller_id: &str,
        spending_card: Option<&CardDefinition>,
        context: &ManaProjectionContext<'_>,
    ) -> Result<usize, EngineError> {
        self.player_index(controller_id)?;
        self.try_maximum_potential_mana_with(
            controller_id,
            spending_card,
            &mut |source, spending| {
                self.checked_mana_activation_plan_in_context(
                    source, spending, false, false, context,
                )
                .map(CheckedManaActivationPlan::into_payment_sources)
            },
        )
    }

    pub(super) fn try_maximum_potential_mana_with<C, E>(
        &self,
        controller_id: &str,
        spending_card: Option<&CardDefinition>,
        resolve: &mut C,
    ) -> Result<usize, E>
    where
        C: FnMut(&CardInstance, Option<&CardDefinition>) -> Result<Vec<ManaChoice>, E>,
    {
        // Preserve the historical invalid-player behavior in the legacy kernel.
        // The checked wrapper validates that identity before reaching this point.
        let Ok(player_index) = self.player_index(controller_id) else {
            return Ok(0);
        };
        let player = &self.state.players[player_index];
        let mut source_maxima = 0;
        for source in player
            .battlefield
            .iter()
            .filter(|source| !self.permanent_cannot_pay_tap_symbol(source))
        {
            if let Some(maximum) = resolve(source, spending_card)?
                .into_iter()
                .map(|choice| choice.symbols.len())
                .max()
            {
                source_maxima += maximum;
            }
        }
        Ok(player.mana_pool.len() + source_maxima)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_declaration_budget_tests.rs"]
mod tests;
