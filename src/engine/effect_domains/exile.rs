//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("exilePermanent") => {
            (target_reference_supported(&effect["permanent"])
                || object_expression_supported(&effect["permanent"])
                || (value_kind(&effect["permanent"]) == Some("chosenTargets")
                    && effect["permanent"]["id"].is_string())
                || (value_kind(&effect["permanent"]) == Some("eachPermanent")
                    && card_filter_supported(&effect["permanent"]["where"])
                    && effect["permanent"]
                        .get("player")
                        .is_none_or(player_reference_supported)
                    && effect["permanent"]
                        .get("excludeSource")
                        .is_none_or(Value::is_boolean)
                    && effect["permanent"]
                        .get("exclude")
                        .is_none_or(object_expression_supported)))
                && effect.get("bind").is_none_or(Value::is_string)
                && effect
                    .get("until")
                    .is_none_or(|until| value_kind(until) == Some("sourceLeavesBattlefield"))
                && effect.get("linkToSource").is_none_or(Value::is_boolean)
                && (value_kind(effect) != Some("destroyPermanent")
                    || effect.get("cannotRegenerate").is_none_or(Value::is_boolean))
        }

        Some("exilePlayerGraveyard") => player_reference_supported(&effect["player"]),
        Some("exileChosenPlayersGraveyards") => effect["targetsDecisionId"].is_string(),

        Some("exileTogether") => object_expression_supported(&effect["objects"]),
        Some("exileTargetCards") => {
            object_expression_supported(&effect["cards"])
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("exileCards") => {
            matches!(integer_value(&effect["count"]), Some(count) if count >= 0)
                && value_kind(&effect["from"]) == Some("graveyard")
                && player_reference_supported(&effect["from"]["player"])
        }
        Some("exileAbilitySourceFromGraveyard") => matches!(
            value_kind(&effect["object"]),
            Some("self" | "abilitySource")
        ),
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_exile_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_exile_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_exile_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("exileCards") => {
                self.exile_cards_from_graveyard(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
            }

            Some("exilePlayerGraveyard") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let mut cards = Vec::new();
                while !self.state.players[player_index].graveyard.is_empty() {
                    cards.push(self.take_zone_card_for_movement(player_index, "graveyard", 0)?);
                }
                for mut card in cards {
                    let card_id = card.instance_id.clone();
                    card.controller = card.owner.clone();
                    card.flags.insert("faceDown".to_string(), false);
                    self.commit_movement_identity(
                        &mut card,
                        &super::super::card_zone_transition::ObjectZone::Exile,
                        super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                    )?;
                    self.state.players[player_index].exile.push(card.card);
                    self.record_event(
                        "cardExiled",
                        Some(player_id.clone()),
                        Some(card_id),
                        json!({ "from": "graveyard", "reason": "effect" }),
                    );
                }
            }
            Some("exileChosenPlayersGraveyards") => {
                let Some(decision_id) = effect["targetsDecisionId"].as_str() else {
                    return Ok(());
                };
                let player_ids = stack_object
                    .targets
                    .iter()
                    .filter(|(target_id, _)| {
                        target_id.as_str() == decision_id
                            || target_id.starts_with(&format!("{decision_id}:"))
                    })
                    .filter_map(|(_, target)| match target {
                        TargetRef::Player { player_id } => Some(player_id.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                for player_id in player_ids {
                    let player_index = self.player_index(&player_id)?;
                    let mut cards = Vec::new();
                    while !self.state.players[player_index].graveyard.is_empty() {
                        cards.push(self.take_zone_card_for_movement(
                            player_index,
                            "graveyard",
                            0,
                        )?);
                    }
                    for mut card in cards {
                        let card_id = card.instance_id.clone();
                        card.controller = card.owner.clone();
                        card.flags.insert("faceDown".to_string(), false);
                        self.commit_movement_identity(
                            &mut card,
                            &super::super::card_zone_transition::ObjectZone::Exile,
                            super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                        )?;
                        self.state.players[player_index].exile.push(card.card);
                        self.record_event(
                            "cardExiled",
                            Some(player_id.clone()),
                            Some(card_id),
                            json!({ "from": "graveyard", "reason": "effect" }),
                        );
                    }
                }
            }
            Some("exilePermanent") => {
                let selected_players = effect["permanent"]
                    .get("player")
                    .map(|player| self.resolve_players(player, stack_object, bindings));
                let instance_ids = match value_kind(&effect["permanent"]) {
                    // Declared target collections: members that became
                    // illegal are unaffected, the others are still exiled
                    // (CR 608.2b).
                    Some("chosenTargets" | "union") => self
                        .resolve_runtime_object_ids_for_effect(
                            &effect["permanent"],
                            stack_object,
                            bindings,
                            decisions,
                        )?
                        .unwrap_or_default(),
                    Some("eachPermanent") => self
                        .state
                        .players
                        .iter()
                        .flat_map(|player| player.battlefield.iter())
                        .filter(|permanent| {
                            selected_players
                                .as_ref()
                                .is_none_or(|players| players.contains(&permanent.controller))
                                && self.permanent_matches_effective_filter(
                                    permanent,
                                    &effect["permanent"]["where"],
                                )
                        })
                        .map(|permanent| permanent.instance_id.clone())
                        .collect(),
                    _ => self
                        .resolve_target(&effect["permanent"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                            _ => None,
                        })
                        .unwrap_or_default(),
                };
                // CR 610.3: `until SOURCE leaves the battlefield` links each
                // exiled object to this source incarnation; the shared
                // linked-return service returns it when that source leaves.
                let until_source_leaves =
                    value_kind(&effect["until"]) == Some("sourceLeavesBattlefield");
                // CR 610.3a/b: if the source already left after the ability
                // was put on the stack, the object doesn't move at all.
                let instance_ids = if until_source_leaves
                    && !self.linked_source_is_same_battlefield_object(&stack_object.card)
                {
                    Vec::new()
                } else {
                    instance_ids
                };
                let mut exiled_ids = Vec::new();
                for instance_id in instance_ids {
                    let battlefield_object =
                        self.permanent_position(&instance_id)
                            .map(|(player_index, card_index)| {
                                let permanent =
                                    &self.state.players[player_index].battlefield[card_index];
                                (permanent.controller.clone(), permanent.definition.clone())
                            });
                    let affected_controller = battlefield_object
                        .as_ref()
                        .map(|(controller, _)| controller.clone());
                    self.exile_permanent(&instance_id)?;
                    let was_exiled = affected_controller.is_some()
                        && self.card_zone_owner_id(&instance_id, "exile").is_some();
                    if was_exiled {
                        exiled_ids.push(instance_id.clone());
                    }
                    // CR 607.2a: a linked ability of this object reads the
                    // cards exiled with it; CR 610.3 also returns them.
                    let link_to_source = effect["linkToSource"].as_bool() == Some(true);
                    if was_exiled
                        && (until_source_leaves || link_to_source)
                        && let Some((_, last_known_definition)) = &battlefield_object
                    {
                        self.register_linked_exiled_permanent(
                            &stack_object.card,
                            &instance_id,
                            until_source_leaves,
                            last_known_definition,
                        )?;
                    }
                    if was_exiled
                        && let Some(affected_controller) = affected_controller
                        && affected_controller != stack_object.controller
                    {
                        self.record_event(
                            "permanentExiledBy",
                            Some(stack_object.controller.clone()),
                            Some(instance_id),
                            json!({ "affectedControllerId": affected_controller }),
                        );
                    }
                }
                if let Some(binding_id) = effect["bind"].as_str() {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Objects(exiled_ids.clone()),
                    );
                }
                if let Some(binding_id) = effect["bindCountAs"].as_str() {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Integer(exiled_ids.len() as i32),
                    );
                }
            }
            Some("exileTogether") => {
                let objects = self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                for instance_id in &objects {
                    if self.permanent_position(instance_id).is_some() {
                        self.exile_permanent(instance_id)?;
                        continue;
                    }
                    let found = self
                        .prepare_card_movement_source(instance_id, None)?
                        .filter(|source| {
                            matches!(
                                source.zone,
                                super::super::card_zone_transition::ObjectZone::Graveyard(_)
                            )
                        })
                        .map(|source| self.take_prepared_movement_card(source))
                        .transpose()?;
                    if let Some(mut card) = found {
                        let owner_index = self.player_index(&card.owner)?;
                        self.commit_movement_identity(
                            &mut card,
                            &super::super::card_zone_transition::ObjectZone::Exile,
                            super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                        )?;
                        self.state.players[owner_index].exile.push(card.card);
                    }
                }
                self.record_event(
                    "cardsExiledTogether",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "cardIds": objects }),
                );
            }

            Some("exileAbilitySourceFromGraveyard") => {
                let source_id = &stack_object.card.instance_id;
                for player_index in 0..self.state.players.len() {
                    if let Some(card_index) = self.state.players[player_index]
                        .graveyard
                        .iter()
                        .position(|card| card.instance_id == *source_id)
                    {
                        let mut card = self.take_zone_card_for_movement(
                            player_index,
                            "graveyard",
                            card_index,
                        )?;
                        let owner_index = self.player_index(&card.owner)?;
                        self.commit_movement_identity(
                            &mut card,
                            &super::super::card_zone_transition::ObjectZone::Exile,
                            super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                        )?;
                        self.state.players[owner_index].exile.push(card.card);
                        self.record_event(
                            "abilitySourceExiled",
                            Some(stack_object.controller.clone()),
                            Some(source_id.clone()),
                            json!({ "from": "graveyard" }),
                        );
                        break;
                    }
                }
            }
            Some("exileTargetCards") => {
                let object_ids = self
                    .resolve_runtime_object_ids(&effect["cards"], stack_object, bindings, decisions)
                    .unwrap_or_default();
                // The exiled cards a later `it` / `that card` names (CR 400.7).
                let mut exiled_card_ids = Vec::new();
                for instance_id in object_ids {
                    if let Some(stack_index) = self
                        .state
                        .stack
                        .iter()
                        .position(|object| object.id == instance_id)
                    {
                        let physical_source = if is_spell_copy(&self.state.stack[stack_index].card)
                        {
                            None
                        } else {
                            Some(self.prepare_stack_card_movement_source(stack_index)?)
                        };
                        let exiled_object = self.take_stack_object_with_lki(stack_index);
                        let card_id = exiled_object.card.instance_id.clone();
                        if !is_spell_copy(&exiled_object.card) {
                            let owner_index = self.player_index(&exiled_object.card.owner)?;
                            let mut card = exiled_object.card;
                            let source = physical_source.as_ref().ok_or_else(|| {
                                EngineError::new("exiled physical spell lacks a source identity")
                            })?;
                            super::super::card_zone_transition::reset_movement_card(
                                &mut card, source,
                            )?;
                            card.controller = card.owner.clone();
                            self.commit_card_zone_transition(
                                &mut card,
                                source,
                                &super::super::card_zone_transition::ObjectZone::Exile,
                                super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                            )?;
                            self.state.players[owner_index].exile.push(card);
                        }
                        self.record_event(
                            "stackObjectExiled",
                            Some(stack_object.controller.clone()),
                            Some(card_id),
                            json!({
                                "source": stack_object.card.instance_id,
                                "stackId": instance_id,
                            }),
                        );
                        continue;
                    }
                    let found = self
                        .prepare_card_movement_source(&instance_id, None)?
                        .map(|source| self.take_prepared_movement_card(source))
                        .transpose()?;
                    if let Some(mut card) = found {
                        let owner_index = self.player_index(&card.owner)?;
                        super::super::card_zone_transition::reset_owned_movement_card(&mut card)?;
                        card.controller = card.owner.clone();
                        self.commit_movement_identity(
                            &mut card,
                            &super::super::card_zone_transition::ObjectZone::Exile,
                            super::super::card_zone_transition::ObjectTransitionReason::Reexile,
                        )?;
                        self.state.players[owner_index].exile.push(card.card);
                        exiled_card_ids.push(instance_id.clone());
                        self.record_event(
                            "cardExiled",
                            Some(stack_object.controller.clone()),
                            Some(instance_id),
                            json!({ "source": stack_object.card.instance_id }),
                        );
                    }
                }
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(
                        binding.to_string(),
                        RuntimeBinding::Objects(exiled_card_ids),
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
