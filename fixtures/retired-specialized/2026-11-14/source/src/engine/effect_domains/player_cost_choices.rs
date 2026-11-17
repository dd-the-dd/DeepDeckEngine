//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("eachPlayerPaysCostOrLosesLife") => {
            resolution_optional_card_cost_supported(&effect["cost"])
                && numeric_expression_supported(&effect["amount"])
        }
        Some("forEachOpponentPaysCostOrControllerEffect") => {
            resolution_optional_card_cost_supported(&effect["cost"])
                && effect.get("bindOpponentAs").is_none_or(Value::is_string)
                && effect["otherwise"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_player_cost_choices_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_player_cost_choices_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_player_cost_choices_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("eachPlayerPaysCostOrLosesLife") => {
                let amount = self
                    .runtime_numeric_expression(
                        &effect["amount"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0)
                    .max(0);
                let source_controller = stack_object.card.controller.clone();
                let opponents_only = value_kind(&effect["players"]) == Some("opponentsOf");
                let player_ids = self
                    .state
                    .players
                    .iter()
                    .filter(|player| {
                        !player.has_lost && (!opponents_only || player.id != source_controller)
                    })
                    .map(|player| player.id.clone())
                    .collect::<Vec<_>>();
                for player_id in player_ids {
                    let player_index = self.player_index(&player_id)?;
                    let cost = &effect["cost"];
                    let candidates = match value_kind(cost) {
                        Some("discardCard") => self.state.players[player_index]
                            .hand
                            .iter()
                            .filter(|card| {
                                cost["where"].is_null()
                                    || matches_card_filter(&card.definition, &cost["where"])
                            })
                            .map(|card| card.instance_id.clone())
                            .collect::<Vec<_>>(),
                        Some("sacrificePermanent") => self.state.players[player_index]
                            .battlefield
                            .iter()
                            .filter(|card| {
                                cost["where"].is_null()
                                    || matches_permanent_filter(card, &cost["where"])
                            })
                            .map(|card| card.instance_id.clone())
                            .collect::<Vec<_>>(),
                        _ => Vec::new(),
                    };
                    let mut options = vec![LegalAction {
                        id: format!("declineRoomCost:{}:{player_id}", stack_object.id),
                        kind: ActionKind::DeclinePayment,
                        player_id: player_id.clone(),
                        label: format!("Lose {amount} life"),
                        card_instance_id: None,
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    }];
                    options.extend(candidates.into_iter().map(|instance_id| LegalAction {
                        id: format!("payRoomCost:{}:{player_id}:{instance_id}", stack_object.id),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: match value_kind(cost) {
                            Some("discardCard") => {
                                format!("Discard {}", self.card_name_for_instance(&instance_id))
                            }
                            _ => format!("Sacrifice {}", self.card_name_for_instance(&instance_id)),
                        },
                        card_instance_id: Some(instance_id),
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    }));
                    let action = self.choose_action(
                        provider,
                        EngineDecisionRequest {
                            id: format!("eachPlayerRoomCost:{}:{player_id}", stack_object.id),
                            kind: DecisionKind::ResolutionChoice,
                            player_id: player_id.clone(),
                            source_card: Some(stack_object.card.clone()),
                            source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                            choice: None,
                            options,
                        },
                    )?;
                    let Some(instance_id) = action.card_instance_id else {
                        self.lose_life(&player_id, amount, &stack_object.card.instance_id)?;
                        continue;
                    };
                    match value_kind(cost) {
                        Some("discardCard") => {
                            if let Some(card_index) = self.state.players[player_index]
                                .hand
                                .iter()
                                .position(|card| card.instance_id == instance_id)
                            {
                                let card = self.state.players[player_index].hand.remove(card_index);
                                self.put_card_into_graveyard(player_index, card);
                                self.record_event(
                                    "cardDiscarded",
                                    Some(player_id.clone()),
                                    Some(instance_id),
                                    json!({ "reason": "optionalCost" }),
                                );
                                self.enqueue_card_discarded_triggers(&player_id);
                            }
                        }
                        Some("sacrificePermanent") => {
                            self.move_permanent_to_graveyard(&instance_id, "sacrificed")?;
                        }
                        _ => {}
                    }
                }
            }
            Some("forEachOpponentPaysCostOrControllerEffect") => {
                let opponent_ids = self
                    .state
                    .players
                    .iter()
                    .filter(|player| !player.has_lost && player.id != stack_object.controller)
                    .map(|player| player.id.clone())
                    .collect::<Vec<_>>();
                for player_id in opponent_ids {
                    let player_index = self.player_index(&player_id)?;
                    let cost = &effect["cost"];
                    let shared_type_definition = (value_kind(&cost["where"])
                        == Some("sharesCardTypeWithBoundObject"))
                    .then(|| {
                        let binding = cost["where"]["binding"].as_str()?;
                        let instance_id = match bindings.get(binding)? {
                            RuntimeBinding::Objects(objects) => objects.first()?,
                            _ => return None,
                        };
                        self.card_outside_battlefield(instance_id)
                            .map(|card| card.definition.clone())
                            .or_else(|| {
                                self.permanent_position(instance_id).map(
                                    |(source_player, source_index)| {
                                        self.state.players[source_player].battlefield[source_index]
                                            .definition
                                            .clone()
                                    },
                                )
                            })
                    })
                    .flatten();
                    let candidates = match value_kind(cost) {
                        Some("discardCard") => self.state.players[player_index]
                            .hand
                            .iter()
                            .filter(|card| {
                                cost["where"].is_null()
                                    || matches_card_filter(&card.definition, &cost["where"])
                            })
                            .map(|card| card.instance_id.clone())
                            .collect::<Vec<_>>(),
                        Some("sacrificePermanent") => self.state.players[player_index]
                            .battlefield
                            .iter()
                            .filter(|card| {
                                if value_kind(&cost["where"])
                                    == Some("sharesCardTypeWithBoundObject")
                                {
                                    shared_type_definition.as_ref().is_some_and(|source| {
                                        [
                                            "Artifact",
                                            "Battle",
                                            "Creature",
                                            "Enchantment",
                                            "Land",
                                            "Planeswalker",
                                        ]
                                        .iter()
                                        .any(|card_type| {
                                            has_card_type(source, card_type)
                                                && has_card_type(&card.definition, card_type)
                                        })
                                    })
                                } else {
                                    cost["where"].is_null()
                                        || matches_permanent_filter(card, &cost["where"])
                                }
                            })
                            .map(|card| card.instance_id.clone())
                            .collect::<Vec<_>>(),
                        _ => Vec::new(),
                    };
                    let mut options = vec![LegalAction {
                        id: format!("declineOpponentCost:{}:{player_id}", stack_object.id),
                        kind: ActionKind::DeclinePayment,
                        player_id: player_id.clone(),
                        label: "Decline the optional cost".to_string(),
                        card_instance_id: None,
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    }];
                    options.extend(candidates.into_iter().map(|instance_id| LegalAction {
                        id: format!(
                            "payOpponentCost:{}:{player_id}:{instance_id}",
                            stack_object.id
                        ),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: match value_kind(cost) {
                            Some("discardCard") => {
                                format!("Discard {}", self.card_name_for_instance(&instance_id))
                            }
                            _ => format!("Sacrifice {}", self.card_name_for_instance(&instance_id)),
                        },
                        card_instance_id: Some(instance_id),
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    }));
                    let action = self.choose_action(
                        provider,
                        EngineDecisionRequest {
                            id: format!("opponentCostOrEffect:{}:{player_id}", stack_object.id),
                            kind: DecisionKind::ResolutionChoice,
                            player_id: player_id.clone(),
                            source_card: Some(stack_object.card.clone()),
                            source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                            choice: None,
                            options,
                        },
                    )?;
                    let Some(instance_id) = action.card_instance_id else {
                        let previous_binding = effect["bindOpponentAs"].as_str().and_then(|id| {
                            bindings
                                .insert(id.to_string(), RuntimeBinding::Player(player_id.clone()))
                        });
                        self.execute_effects(
                            &effect["otherwise"],
                            stack_object,
                            bindings,
                            decisions,
                            provider,
                        )?;
                        if let Some(binding_id) = effect["bindOpponentAs"].as_str() {
                            if let Some(previous) = previous_binding {
                                bindings.insert(binding_id.to_string(), previous);
                            } else {
                                bindings.remove(binding_id);
                            }
                        }
                        continue;
                    };
                    match value_kind(cost) {
                        Some("discardCard") => {
                            if let Some(card_index) = self.state.players[player_index]
                                .hand
                                .iter()
                                .position(|card| card.instance_id == instance_id)
                            {
                                let card = self.state.players[player_index].hand.remove(card_index);
                                self.put_card_into_graveyard(player_index, card);
                                self.record_event(
                                    "cardDiscarded",
                                    Some(player_id.clone()),
                                    Some(instance_id),
                                    json!({ "reason": "optionalCost" }),
                                );
                                self.enqueue_card_discarded_triggers(&player_id);
                            }
                        }
                        Some("sacrificePermanent") => {
                            self.move_permanent_to_graveyard(&instance_id, "sacrificed")?;
                        }
                        _ => {}
                    }
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
