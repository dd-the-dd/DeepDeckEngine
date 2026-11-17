//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("discardCards") => {
            player_reference_supported(&effect["player"])
                && match (effect.get("cards"), effect.get("count")) {
                    (Some(cards), None) => object_expression_supported(cards),
                    (None, Some(count)) => numeric_expression_supported(count),
                    _ => false,
                }
                && effect.get("where").is_none_or(card_filter_supported)
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("discardHand") => {
            player_reference_supported(&effect["player"])
                && effect.get("bindCountAs").is_none_or(Value::is_string)
        }
        Some("exileCardsFromHand") => {
            player_reference_supported(&effect["player"])
                && numeric_expression_supported(&effect["count"])
        }
        Some("lookAtRandomCardInHand") => {
            player_reference_supported(&effect["viewer"])
                && player_reference_supported(&effect["handOwner"])
        }

        Some("revealHand") => {
            player_reference_supported(&effect["player"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && effect.get("continuous").is_none_or(Value::is_boolean)
        }
        Some("lookAtHand") => {
            player_reference_supported(&effect["player"])
                && player_reference_supported(&effect["viewer"])
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_hand_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_hand_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_hand_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("discardCards") => {
                let player_ids = match value_kind(&effect["player"]) {
                    Some("opponentsOf") => self
                        .state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost && player.id != stack_object.controller)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>(),
                    Some("eachPlayer") => self
                        .state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>(),
                    _ => self
                        .resolve_player(&effect["player"], stack_object, bindings)
                        .into_iter()
                        .collect::<Vec<_>>(),
                };
                let mut discarded_ids = Vec::new();
                for player_id in player_ids {
                    let player_index = self.player_index(&player_id)?;
                    let selected = if let Some(cards) = effect.get("cards") {
                        self.resolve_runtime_object_ids(cards, stack_object, bindings, decisions)
                            .unwrap_or_default()
                    } else {
                        let count = self
                            .checked_runtime_numeric_expression(
                                &effect["count"],
                                stack_object,
                                bindings,
                                decisions,
                                "discardCards.count",
                            )?
                            .max(0) as usize;
                        let candidates = self.state.players[player_index]
                            .hand
                            .iter()
                            .filter(|card| {
                                effect.get("where").is_none_or(|where_filter| {
                                    matches_card_filter(&card.definition, where_filter)
                                })
                            })
                            .map(|card| card.instance_id.clone())
                            .collect::<Vec<_>>();
                        let player_count = count.min(candidates.len());
                        if player_count == 0 {
                            continue;
                        }
                        self.choose_resolution_objects(
                            stack_object,
                            provider,
                            &player_id,
                            "discardCards",
                            candidates,
                            player_count,
                            player_count,
                            &format!("Choose {player_count} card(s) to discard"),
                        )?
                    };
                    let mut graveyard_destinations = selected
                        .iter()
                        .filter(|id| {
                            self.state.players[player_index]
                                .hand
                                .iter()
                                .any(|card| &card.instance_id == *id)
                        })
                        .map(|id| {
                            self.prepare_card_graveyard_destination(id)
                                .map(|plan| (id.clone(), plan))
                        })
                        .collect::<Result<BTreeMap<_, _>, EngineError>>()?;
                    for instance_id in selected {
                        if let Some(card_index) = self.state.players[player_index]
                            .hand
                            .iter()
                            .position(|card| card.instance_id == instance_id)
                        {
                            let card =
                                self.take_zone_card_for_movement(player_index, "hand", card_index)?;
                            let plan =
                                graveyard_destinations.remove(&instance_id).ok_or_else(|| {
                                    EngineError::new("discarded card destination was not prepared")
                                })?;
                            self.put_owned_card_into_prepared_graveyard(player_index, card, plan)?;
                            self.record_event(
                                "cardDiscarded",
                                Some(player_id.clone()),
                                Some(instance_id.clone()),
                                json!({
                                    "reason": "ability",
                                    "sourceControllerId": stack_object.controller,
                                }),
                            );
                            self.enqueue_card_discarded_triggers(&player_id);
                            discarded_ids.push(instance_id);
                        }
                    }
                }
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(discarded_ids));
                }
            }
            Some("exileCardsFromHand") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let requested = self
                    .runtime_numeric_expression(&effect["count"], stack_object, bindings, decisions)
                    .unwrap_or(0)
                    .max(0) as usize;
                let count = requested.min(self.state.players[player_index].hand.len());
                let candidates = self.state.players[player_index]
                    .hand
                    .iter()
                    .map(|card| card.instance_id.clone())
                    .collect::<Vec<_>>();
                let selected = self.choose_resolution_objects(
                    stack_object,
                    provider,
                    &player_id,
                    "cardsToExileFromHand",
                    candidates,
                    count,
                    count,
                    "Choose cards from your hand to exile.",
                )?;
                for instance_id in selected {
                    if let Some(card_index) = self.state.players[player_index]
                        .hand
                        .iter()
                        .position(|card| card.instance_id == instance_id)
                    {
                        let mut card =
                            self.take_zone_card_for_movement(player_index, "hand", card_index)?;
                        self.commit_movement_identity(
                            &mut card,
                            &super::super::card_zone_transition::ObjectZone::Exile,
                            super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                        )?;
                        self.state.players[player_index].exile.push(card.card);
                        self.record_event(
                            "cardExiledFromHand",
                            Some(player_id.clone()),
                            Some(instance_id),
                            json!({ "source": stack_object.card.instance_id }),
                        );
                    }
                }
            }
            Some("discardHand") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let discarded_count = self.state.players[player_index].hand.len() as i32;
                let mut graveyard_destinations = self.state.players[player_index]
                    .hand
                    .iter()
                    .map(|card| {
                        self.prepare_card_graveyard_destination(&card.instance_id)
                            .map(|plan| (card.instance_id.clone(), plan))
                    })
                    .collect::<Result<BTreeMap<_, _>, EngineError>>()?;
                let mut discarded = Vec::new();
                while !self.state.players[player_index].hand.is_empty() {
                    discarded.push(self.take_zone_card_for_movement(player_index, "hand", 0)?);
                }
                for card in discarded {
                    let instance_id = card.instance_id.clone();
                    let plan = graveyard_destinations.remove(&instance_id).ok_or_else(|| {
                        EngineError::new("discarded hand destination was not prepared")
                    })?;
                    self.put_owned_card_into_prepared_graveyard(player_index, card, plan)?;
                    self.record_event(
                        "cardDiscarded",
                        Some(player_id.clone()),
                        Some(instance_id),
                        json!({
                            "reason": "ability",
                            "sourceControllerId": stack_object.controller,
                        }),
                    );
                    self.enqueue_card_discarded_triggers(&player_id);
                }
                if let Some(binding) = effect["bindCountAs"].as_str() {
                    bindings.insert(
                        binding.to_string(),
                        RuntimeBinding::Integer(discarded_count),
                    );
                }
            }
            Some("lookAtRandomCardInHand") => {
                let Some(viewer_id) =
                    self.resolve_player(&effect["viewer"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(hand_owner_id) =
                    self.resolve_player(&effect["handOwner"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let hand_owner_index = self.player_index(&hand_owner_id)?;
                let viewed_card_id = self.state.players[hand_owner_index]
                    .hand
                    .choose(&mut self.rng)
                    .map(|card| card.instance_id.clone());
                self.record_event(
                    "randomHandCardLookedAt",
                    Some(viewer_id.clone()),
                    viewed_card_id.clone(),
                    json!({
                        "cardId": viewed_card_id,
                        "handOwnerId": hand_owner_id,
                        "privateToPlayerId": viewer_id,
                    }),
                );
            }

            Some("revealHand") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.remember_revealed_hand(&player_id);
                    if effect["continuous"].as_bool() == Some(true) {
                        self.state.rule_modifiers.push(json!({
                            "kind": "revealedHand",
                            "playerId": player_id,
                            "expiresAfterTurn": self.state.turn_number,
                            "sourceCardInstanceId": stack_object.card.instance_id,
                        }));
                    }
                }
            }
            Some("lookAtHand") => {
                let hand_owner = self.resolve_player(&effect["player"], stack_object, bindings);
                let viewer = self.resolve_player(&effect["viewer"], stack_object, bindings);
                if let (Some(hand_owner), Some(viewer)) = (hand_owner, viewer) {
                    self.remember_looked_at_hand(&hand_owner, &viewer);
                    self.record_event(
                        "handLookedAt",
                        Some(viewer),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "handOwnerId": hand_owner }),
                    );
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
