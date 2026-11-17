//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("optionalPayLife") => {
            player_reference_supported(&effect["player"])
                && integer_value(&effect["amount"]).is_some_and(|amount| amount > 0)
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_payments_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_payments_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_payments_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("optionalPayLife") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let amount = integer_value(&effect["amount"]).unwrap_or(0).max(0);
                let can_pay = self
                    .player_index(&player_id)
                    .ok()
                    .is_some_and(|index| self.state.players[index].life >= amount);
                if !can_pay
                    || !self.confirm_resolution_action(
                        stack_object,
                        provider,
                        &player_id,
                        &format!("Pay {amount} life?"),
                    )?
                {
                    return Ok(());
                }
                self.lose_life(&player_id, amount, &stack_object.card.instance_id)?;
                let effects = effect["effects"].as_array().cloned().unwrap_or_default();
                for (index, nested_effect) in effects.iter().enumerate() {
                    self.execute_effect(
                        nested_effect,
                        effects.get(index + 1),
                        stack_object,
                        bindings,
                        decisions,
                        provider,
                    )?;
                }
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    value_kind(effect).unwrap_or("missingKind")
                )));
            }
        }
        Ok(())
    }
}
