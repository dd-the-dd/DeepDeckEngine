//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("grantExtraTurn") => player_reference_supported(&effect["player"]),
        Some("grantNoMaximumHandSize") => player_reference_supported(&effect["player"]),
        Some("loseGame") => player_reference_supported(&effect["player"]),

        Some("ventureDungeon") => {
            player_reference_supported(&effect["player"])
                && effect.get("procedure").is_none_or(Value::is_string)
        }
        Some("restrictPlayerActions") => {
            player_reference_supported(&effect["player"])
                && effect["castSpells"].is_boolean()
                && effect["activateAbilities"].is_boolean()
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_player_status_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        _decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_player_status_domain_known(effect, stack_object, bindings, provider)
            .map(|()| true)
    }

    fn execute_player_status_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("grantNoMaximumHandSize") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let modifier = json!({
                    "kind": "noMaximumHandSize",
                    "playerId": player_id,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "noMaximumHandSizeGranted",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("loseGame") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                    && self
                        .state
                        .players
                        .iter()
                        .any(|player| player.id == player_id && !player.has_lost)
                {
                    self.record_event(
                        "playerLostGame",
                        Some(player_id.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "reason": "effect" }),
                    );
                    self.eliminate_players(GameEndReason::LifeTotal, vec![player_id]);
                }
            }

            Some("grantExtraTurn") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "extraTurnForPlayer",
                        "playerId": player_id,
                        "afterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("ventureDungeon") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    // CR 701.49c: a named procedure enters only its Dungeon.
                    let procedure = effect["procedure"].as_str();
                    self.venture_into_dungeon(stack_object, provider, &player_id, procedure)?;
                }
            }
            Some("restrictPlayerActions") => {
                let player_ids = if value_kind(&effect["player"]) == Some("eachPlayer") {
                    self.state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>()
                } else {
                    self.resolve_player(&effect["player"], stack_object, bindings)
                        .into_iter()
                        .collect()
                };
                for player_id in player_ids {
                    self.state.rule_modifiers.push(json!({
                        "kind": "playerActionRestriction",
                        "playerId": player_id,
                        "castSpells": effect["castSpells"],
                        "activateAbilities": effect["activateAbilities"],
                        "expiresAfterTurn": self.state.turn_number,
                        "sourceCardInstanceId": stack_object.card.instance_id,
                    }));
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
