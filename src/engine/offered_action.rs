//! `any [other] player|opponent may INSTRUCTION. If a player does, A. If no
//! one does, B.` (CR 608.2d, 101.4). `offerAction` offers one instruction to
//! each player of a population in turn order, starting with the active
//! player. The offered player decides; the instruction then runs as that
//! player's own (every `you` and choice inside it is theirs, as for a
//! performer, CR 608.2c), and its actual performance is measured with the
//! shared optional-action witness. The first player who performs it ends the
//! offer and `onPerformed` resolves; when nobody performs it, `onDeclined`
//! resolves. Both outcomes are the ability controller's instructions.

use super::*;

/// `Some(...)` for an offer: a supported population, one measured
/// instruction and supported outcome programs.
pub(super) fn offered_action_supported(effect: &Value) -> Option<bool> {
    if value_kind(effect) != Some("offerAction") {
        return None;
    }
    let outcome_supported = |key: &str| {
        effect.get(key).is_none_or(|effects| {
            effects
                .as_array()
                .is_some_and(|effects| effects.iter().all(effect_supported))
        })
    };
    Some(
        (player_set_reference_supported(&effect["players"])
            || player_complement_reference_supported(&effect["players"]))
            && effect.get("performer").is_none()
            && effect_supported(&effect["action"])
            && optional_action::optional_action_witness_supported(&effect["action"])
            && (effect.get("onPerformed").is_some() || effect.get("onDeclined").is_some())
            && outcome_supported("onPerformed")
            && outcome_supported("onDeclined"),
    )
}

impl GameEngine {
    /// Runs an offer. `Ok(false)` when the effect is no offer.
    pub(super) fn execute_offered_action<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("offerAction") {
            return Ok(false);
        }
        let players = self.player_iteration_ids(&effect["players"], stack_object, bindings)?;
        let mut performed_by = None;
        for player_id in players {
            // The instruction is the offered player's own (CR 608.2c).
            let mut offered = stack_object.clone();
            offered.controller = player_id.clone();
            // CR 608.2d: an impossible instruction can't be chosen.
            if !self.optional_action_is_available(&effect["action"], &offered, bindings) {
                continue;
            }
            if !self.offered_player_accepts(&player_id, stack_object, provider)? {
                continue;
            }
            let witness = self.prepare_optional_action_witness(
                &effect["action"],
                &offered,
                bindings,
                decisions,
            );
            let first_event = self.state.events.len();
            self.execute_effects(
                &Value::Array(vec![effect["action"].clone()]),
                &offered,
                bindings,
                decisions,
                provider,
            )?;
            if self.optional_action_was_performed(&witness, first_event) {
                performed_by = Some(player_id);
                break;
            }
        }
        self.record_event(
            "offeredActionResolved",
            performed_by.clone(),
            Some(stack_object.card.instance_id.clone()),
            json!({ "performed": performed_by.is_some(), "stackId": stack_object.id }),
        );
        let outcome = if performed_by.is_some() {
            effect.get("onPerformed")
        } else {
            effect.get("onDeclined")
        };
        if let Some(outcome) = outcome {
            self.execute_effects(outcome, stack_object, bindings, decisions, provider)?;
        }
        Ok(true)
    }

    /// The offered player's decision to perform the instruction.
    fn offered_player_accepts<P: DecisionProvider>(
        &mut self,
        player_id: &str,
        stack_object: &StackObject,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        let decision_id = format!(
            "offerAction:{}:{}:{}",
            stack_object.id,
            player_id,
            self.state.events.len()
        );
        let choice_key = "performOfferedAction".to_string();
        let options = [
            (false, "Decline the offered action"),
            (true, "Perform the offered action"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (perform, label))| LegalAction {
            id: format!("{decision_id}:{index}"),
            kind: ActionKind::ChooseResolution,
            player_id: player_id.to_string(),
            label: label.to_string(),
            card_instance_id: Some(stack_object.card.instance_id.clone()),
            payment_sources: Vec::new(),
            decisions: BTreeMap::from([(choice_key.clone(), Value::Bool(perform))]),
            targets: BTreeMap::new(),
            target_order: Vec::new(),
            attacker_id: None,
            blocker_id: None,
        })
        .collect();
        let choice = self.choose_action(
            provider,
            EngineDecisionRequest {
                id: decision_id,
                kind: DecisionKind::ResolutionChoice,
                player_id: player_id.to_string(),
                source_card: Some(stack_object.card.clone()),
                source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                choice: None,
                options,
            },
        )?;
        Ok(choice
            .decisions
            .get(&choice_key)
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }
}
