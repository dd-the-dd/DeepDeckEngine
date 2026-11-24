//! A player's rules state changed by a one-shot instruction: a player
//! designation (CR 724 the monarch), the Ring's temptation (CR 701.54) and a
//! skipped part of the player's future turns (CR 500.11).
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(match value_kind(effect) {
        Some("becomeMonarch" | "temptWithRing") => player_reference_supported(&effect["player"]),
        Some("skipTurnPart") => {
            player_reference_supported(&effect["player"])
                && matches!(
                    effect["part"].as_str(),
                    Some("turn" | "untapStep" | "drawStep" | "combatPhase")
                )
                && effect["count"].as_u64().is_some_and(|count| count > 0)
                && effect["thisTurn"].is_boolean()
                && (effect["part"].as_str() != Some("turn")
                    || effect["thisTurn"].as_bool() == Some(false))
        }
        _ => false,
    })
}

impl GameEngine {
    pub(super) fn execute_player_designations_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        _decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        let Some(player_id) = self.resolve_player(&effect["player"], stack_object, bindings) else {
            return Ok(true);
        };
        match value_kind(effect) {
            // CR 724.4: the player becomes the monarch; the previous
            // monarch, if any, stops being the monarch.
            Some("becomeMonarch") => self.become_monarch(&player_id),
            Some("temptWithRing") => {
                self.tempt_player_with_ring(&player_id, stack_object, provider)?;
            }
            Some("skipTurnPart") => {
                let part = effect["part"].as_str().unwrap_or("turn").to_string();
                self.schedule_skipped_turn_part(
                    &player_id,
                    &part,
                    effect["count"].as_u64().unwrap_or(1),
                    effect["thisTurn"].as_bool().unwrap_or(false),
                );
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    value_kind(effect).unwrap_or("missingKind")
                )));
            }
        }
        Ok(true)
    }

    /// CR 701.54a: the player chooses a creature they control as their
    /// Ring-bearer (they must choose one if they control a creature; they
    /// may keep the same one), then the Ring gains its next ability, up to
    /// four (CR 701.54c). `Whenever the Ring tempts you` triggers.
    fn tempt_player_with_ring<P: DecisionProvider>(
        &mut self,
        player_id: &str,
        stack_object: &StackObject,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let player_index = self.player_index(player_id)?;
        let candidates = self.state.players[player_index]
            .battlefield
            .iter()
            .filter(|card| card.controller == player_id && self.permanent_is_creature(card))
            .map(|card| card.instance_id.clone())
            .collect::<Vec<_>>();
        let bearer_id = if candidates.is_empty() {
            None
        } else {
            let values = Self::resolution_selections(&candidates, 1, 128);
            let mut local_decisions = BTreeMap::new();
            self.make_resolution_decision(
                "chooseRingBearer",
                player_id,
                stack_object,
                DecisionChoice::CardSelection {
                    decision_id: "chooseRingBearer".to_string(),
                    candidate_card_instance_ids: candidates,
                    minimum: 1,
                    maximum: 1,
                    prompt: "Choose your Ring-bearer".to_string(),
                },
                values,
                &mut local_decisions,
                provider,
            )?;
            local_decisions["chooseRingBearer"]
                .as_array()
                .and_then(|selected| selected.first())
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        };
        let existing = self.state.rule_modifiers.iter().position(|modifier| {
            value_kind(modifier) == Some("ringEmblem")
                && modifier["playerId"].as_str() == Some(player_id)
        });
        let level = existing
            .and_then(|index| self.state.rule_modifiers[index]["level"].as_i64())
            .unwrap_or(0)
            .saturating_add(1)
            .min(4);
        let modifier = json!({
            "kind": "ringEmblem",
            "playerId": player_id,
            "level": level,
            "bearerId": bearer_id,
        });
        if let Some(index) = existing {
            self.state.rule_modifiers[index] = modifier;
        } else {
            self.state.rule_modifiers.push(modifier);
        }
        self.record_event(
            "ringTempted",
            Some(player_id.to_string()),
            bearer_id,
            json!({ "level": level }),
        );
        self.enqueue_ring_tempted_triggers(player_id);
        Ok(())
    }
}
