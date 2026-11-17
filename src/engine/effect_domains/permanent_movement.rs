//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::card_zone_transition::{ObjectTransitionReason, ObjectZone};
use super::super::*;

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    matches!(
        value_kind(value),
        Some(
            "movePermanentsToOwnersLibraries"
                | "returnPermanentsToOwnersHands"
                | "returnToOwnersHand"
                | "putPermanentOnOwnerLibrary"
                | "blinkPermanent"
                | "blinkPermanents"
                | "putAbilitySourceOnOwnersLibrary"
                | "putPermanentOnBottomOwnersLibrary"
        )
    )
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("movePermanentsToOwnersLibraries") => {
            card_filter_supported(&effect["where"])
                && matches!(effect["position"].as_str(), Some("top" | "bottom"))
        }

        Some("returnPermanentsToOwnersHands") => card_filter_supported(&effect["where"]),
        Some("returnToOwnersHand") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"])
                || object_collection_expression_supported(&effect["object"])
                || target_reference_supported(&effect["permanent"]))
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("putPermanentOnOwnerLibrary") => {
            target_reference_supported(&effect["permanent"])
                && matches!(
                    effect["position"].as_str(),
                    Some("top" | "bottom" | "ownerChoiceTopOrBottom")
                )
        }
        Some("blinkPermanent") => {
            target_reference_supported(&effect["permanent"])
                && effect
                    .get("controller")
                    .is_none_or(player_reference_supported)
                && effect.get("tapped").is_none_or(Value::is_boolean)
        }
        Some("blinkPermanents") => {
            object_expression_supported(&effect["objects"])
                && numeric_expression_supported(&effect["repeat"])
        }
        Some("putAbilitySourceOnOwnersLibrary") => true,
        Some("putPermanentOnBottomOwnersLibrary") => target_reference_supported(&effect["object"]),
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_permanent_movement_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_permanent_movement_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_permanent_movement_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("blinkPermanent") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let owner_id = self.state.players[player_index].battlefield[card_index]
                    .owner
                    .clone();
                let return_controller = effect
                    .get("controller")
                    .and_then(|player| self.resolve_player(player, stack_object, bindings))
                    .unwrap_or_else(|| owner_id.clone());
                self.exile_permanent(&instance_id)?;
                let owner_index = self.player_index(&owner_id)?;
                let constraint = ("exile".to_string(), owner_index);
                let Some(source) =
                    self.prepare_card_movement_source(&instance_id, Some(&constraint))?
                else {
                    return Ok(());
                };
                let entry_policy = self.capture_zone_entry_policy();
                let definition = &self
                    .card_outside_battlefield(&instance_id)
                    .ok_or_else(|| EngineError::new("blink return source disappeared"))?
                    .definition;
                if entry_policy.admission(definition, "exile").is_prohibited() {
                    return Ok(());
                }
                let replace_with_exile =
                    entry_policy.destination_replacement(definition) == Some("exile");
                let mut card = self.take_prepared_movement_card(source)?;
                if replace_with_exile {
                    card.card.controller = card.card.owner.clone();
                    self.commit_movement_identity(
                        &mut card,
                        &ObjectZone::Exile,
                        ObjectTransitionReason::Reexile,
                    )?;
                    self.state.players[owner_index].exile.push(card.card);
                    return Ok(());
                }
                card.controller = return_controller.clone();
                card.summoning_sick = true;
                card.tapped = effect["tapped"].as_bool().unwrap_or(false);
                self.apply_enter_replacements(&mut card, provider)?;
                self.commit_movement_identity(
                    &mut card,
                    &ObjectZone::Battlefield,
                    ObjectTransitionReason::Move,
                )?;
                let trigger_source = card.card.clone();
                let controller_index = self.player_index(&return_controller)?;
                self.state.players[controller_index]
                    .battlefield
                    .push(card.card);
                self.record_event(
                    "permanentBlinked",
                    Some(return_controller),
                    Some(instance_id),
                    json!({}),
                );
                self.enqueue_enter_battlefield_triggers(&trigger_source);
            }
            Some("blinkPermanents") => {
                let instance_ids = self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                let repeat = self
                    .runtime_numeric_expression(
                        &effect["repeat"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(1)
                    .max(0);
                for _ in 0..repeat {
                    let mut exiled = Vec::new();
                    for instance_id in &instance_ids {
                        let Some((player_index, card_index)) = self.permanent_position(instance_id)
                        else {
                            continue;
                        };
                        let owner_id = self.state.players[player_index].battlefield[card_index]
                            .owner
                            .clone();
                        self.exile_permanent(instance_id)?;
                        exiled.push((instance_id.clone(), owner_id));
                    }
                    for (instance_id, owner_id) in exiled {
                        let owner_index = self.player_index(&owner_id)?;
                        let constraint = ("exile".to_string(), owner_index);
                        let Some(source) =
                            self.prepare_card_movement_source(&instance_id, Some(&constraint))?
                        else {
                            continue;
                        };
                        let entry_policy = self.capture_zone_entry_policy();
                        let definition = &self
                            .card_outside_battlefield(&instance_id)
                            .ok_or_else(|| EngineError::new("blink return source disappeared"))?
                            .definition;
                        if entry_policy.admission(definition, "exile").is_prohibited() {
                            continue;
                        }
                        let replace_with_exile =
                            entry_policy.destination_replacement(definition) == Some("exile");
                        let mut card = self.take_prepared_movement_card(source)?;
                        if replace_with_exile {
                            card.card.controller = card.card.owner.clone();
                            self.commit_movement_identity(
                                &mut card,
                                &ObjectZone::Exile,
                                ObjectTransitionReason::Reexile,
                            )?;
                            self.state.players[owner_index].exile.push(card.card);
                            continue;
                        }
                        card.controller = owner_id.clone();
                        card.summoning_sick = true;
                        self.apply_enter_replacements(&mut card, provider)?;
                        self.commit_movement_identity(
                            &mut card,
                            &ObjectZone::Battlefield,
                            ObjectTransitionReason::Move,
                        )?;
                        let trigger_source = card.card.clone();
                        self.state.players[owner_index].battlefield.push(card.card);
                        self.record_event(
                            "permanentBlinked",
                            Some(owner_id),
                            Some(instance_id),
                            json!({}),
                        );
                        self.enqueue_enter_battlefield_triggers(&trigger_source);
                    }
                }
            }
            Some("returnToOwnersHand") => {
                if object_collection_expression_supported(&effect["object"]) {
                    let mut returned = Vec::new();
                    // Targets that became illegal are not affected, but the
                    // remaining legal targets still are (CR 608.2b).
                    for instance_id in self
                        .resolve_runtime_object_ids_for_effect(
                            &effect["object"],
                            stack_object,
                            bindings,
                            decisions,
                        )?
                        .unwrap_or_default()
                    {
                        self.return_permanent_to_hand(&instance_id)?;
                        if self.card_zone_owner_id(&instance_id, "hand").is_some() {
                            returned.push(instance_id);
                        }
                    }
                    if let Some(binding) = effect["bind"].as_str() {
                        bindings.insert(binding.to_string(), RuntimeBinding::Objects(returned));
                    }
                    return Ok(());
                }
                if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    self.return_permanent_to_hand(&stack_object.card.instance_id)?;
                    return Ok(());
                }
                if let Some(target) = self
                    .resolve_target(&effect["object"], stack_object, bindings)
                    .or_else(|| self.resolve_target(&effect["permanent"], stack_object, bindings))
                {
                    match target {
                        TargetRef::Permanent { instance_id } => {
                            self.return_permanent_to_hand(&instance_id)?;
                        }
                        TargetRef::StackObject { stack_id } => {
                            self.return_stack_object_to_hand(&stack_id)?;
                        }
                        TargetRef::Player { .. } | TargetRef::Card { .. } => {}
                    }
                }
            }
            Some("putPermanentOnOwnerLibrary") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let owner_id = self.state.players[player_index].battlefield[card_index]
                    .owner
                    .clone();
                let position = if effect["position"].as_str() == Some("ownerChoiceTopOrBottom") {
                    self.choose_resolution_option(
                        stack_object,
                        provider,
                        &owner_id,
                        vec!["top".to_string(), "bottom".to_string()],
                        "Choose where to put the permanent in your library.",
                    )?
                    .unwrap_or_else(|| "top".to_string())
                } else {
                    effect["position"].as_str().unwrap_or("top").to_string()
                };
                self.put_permanent_on_owner_library(&instance_id, &position)?;
            }
            Some("movePermanentsToOwnersLibraries") => {
                let ids = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|card| matches_permanent_filter(card, &effect["where"]))
                    .map(|card| card.instance_id.clone())
                    .collect::<Vec<_>>();
                for instance_id in ids {
                    let position = effect["position"].as_str().unwrap_or("bottom");
                    self.put_permanent_on_owner_library(&instance_id, position)?;
                }
            }

            Some("returnPermanentsToOwnersHands") => {
                let ids = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| matches_permanent_filter(permanent, &effect["where"]))
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                for instance_id in ids {
                    self.return_permanent_to_hand(&instance_id)?;
                }
            }
            Some("putAbilitySourceOnOwnersLibrary") => {
                let instance_id = stack_object.card.instance_id.clone();
                if self.permanent_position(&instance_id).is_none() {
                    return Ok(());
                }
                self.put_permanent_on_owner_library(&instance_id, "top")?;
                self.record_event(
                    "abilitySourcePutOnLibrary",
                    Some(stack_object.controller.clone()),
                    Some(instance_id),
                    json!({ "position": "top" }),
                );
            }
            Some("putPermanentOnBottomOwnersLibrary") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                if self.permanent_position(&instance_id).is_none() {
                    return Ok(());
                }
                self.put_permanent_on_owner_library(&instance_id, "bottom")?;
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
