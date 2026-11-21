//! Structured resolution cost control; mana uses the shared typed Result service.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("tryPayCost") => {
            effect["id"].as_str().is_some_and(|id| !id.is_empty())
                && singular_player_reference_supported(&effect["player"])
                && super::super::resolution_costs::cost_program_supported(&effect["cost"])
        }
        Some("optionalPayCostPerformEffects") => {
            player_reference_supported(&effect["player"])
                && super::super::resolution_costs::cost_program_supported(&effect["cost"])
                && effect["effects"]
                    .as_array()
                    .is_some_and(|effects| effects.iter().all(effect_supported))
                && effect["declineEffects"]
                    .as_array()
                    .is_none_or(|effects| effects.iter().all(effect_supported))
                && (effect["effects"]
                    .as_array()
                    .is_some_and(|effects| !effects.is_empty())
                    || effect["declineEffects"]
                        .as_array()
                        .is_some_and(|effects| !effects.is_empty()))
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_cost_transactions_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_cost_transactions_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_cost_transactions_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("tryPayCost") => {
                let id = effect["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| EngineError::new("cost result requires a nonempty local id"))?;
                if decisions.contains_key(id) {
                    return Err(EngineError::new(format!(
                        "duplicate or captured cost result id {id}"
                    )));
                }
                let payer = self
                    .resolve_player(&effect["player"], stack_object, bindings)
                    .ok_or_else(|| {
                        EngineError::new("cost program has no resolved singular payer")
                    })?;
                let program = super::super::resolution_costs::CostProgram::decode(&effect["cost"])?;
                let prepared = self.prepare_resolution_cost_payment(
                    &payer,
                    program,
                    stack_object,
                    bindings,
                    decisions,
                )?;
                let result = self.choose_and_commit_resolution_payment(
                    prepared,
                    Some(id),
                    stack_object,
                    provider,
                )?;
                decisions.insert(id.to_string(), Value::Bool(result.paid()));
            }
            Some("optionalPayCostPerformEffects") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let program = super::super::resolution_costs::CostProgram::decode(&effect["cost"])?;
                let prepared = self.prepare_resolution_cost_payment(
                    &player_id,
                    program,
                    stack_object,
                    bindings,
                    decisions,
                )?;
                let outcome = self.choose_and_commit_resolution_payment(
                    prepared,
                    None,
                    stack_object,
                    provider,
                )?;
                let branch = if outcome.paid() {
                    &effect["effects"]
                } else {
                    &effect["declineEffects"]
                };
                self.execute_effects(branch, stack_object, bindings, decisions, provider)?;
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
