//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("exchangeLifeWithSourcePower") => player_reference_supported(&effect["player"]),
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_life_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        _provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_life_domain_known(effect, stack_object, bindings, decisions)
            .map(|()| true)
    }

    fn execute_life_domain_known(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("exchangeLifeWithSourcePower") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((source_player, source_index)) =
                    self.permanent_position(&stack_object.card.instance_id)
                else {
                    return Ok(());
                };
                let source_power = self
                    .current_power(&self.state.players[source_player].battlefield[source_index]);
                let player_index = self.player_index(&player_id)?;
                let previous_life = self.state.players[player_index].life;
                self.state.players[player_index].life = source_power;
                self.state.players[source_player].battlefield[source_index].power_modifier +=
                    previous_life - source_power;
                self.record_event(
                    "lifePowerExchanged",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "life": source_power, "power": previous_life }),
                );
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
