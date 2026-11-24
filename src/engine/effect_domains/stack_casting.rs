//! Stack actions and independently configured counter destinations.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("attachPermanent") => {
            (matches!(
                value_kind(&effect["attachment"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["attachment"])
                || object_expression_supported(&effect["attachment"]))
                && (target_reference_supported(&effect["to"])
                    || value_kind(&effect["to"]) == Some("decisionResult")
                        && effect["to"]["decisionId"].is_string()
                    || object_expression_supported(&effect["to"]))
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("castOneCard") => {
            player_reference_supported(&effect["player"])
                && object_expression_supported(&effect["cards"])
                && matches!(
                    effect["sourceZone"].as_str(),
                    Some("graveyard" | "hand" | "exile")
                )
                && effect["normalCostsApply"].is_boolean()
                && effect["optional"].is_boolean()
                && effect["bind"].is_string()
                && effect.get("spellWhere").is_none_or(stack_filter_supported)
        }
        Some("castAnyNumber") => {
            cast_any_number_supported(effect) && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("counterSpell") => {
            target_reference_supported(&effect["spell"])
                && effect.get("exileInstead").is_none_or(Value::is_boolean)
                && effect
                    .get("exileInsteadWhere")
                    .is_none_or(card_filter_supported)
                && effect.get("bindExiledAs").is_none_or(Value::is_string)
                && effect
                    .get("prohibitControllerSpellsThisTurn")
                    .is_none_or(Value::is_boolean)
                && effect.get("addManaAtNextMainPhase").is_none_or(|symbol| {
                    symbol
                        .as_str()
                        .is_some_and(|symbol| matches!(symbol, "W" | "U" | "B" | "R" | "G" | "C"))
                })
                && effect.get("controllerCreatesTokens").is_none_or(|tokens| {
                    tokens["name"].is_string() && numeric_expression_supported(&tokens["quantity"])
                })
        }
        Some("counterStackObject") => {
            target_reference_supported(&effect["object"])
                && effect.get("exileInstead").is_none_or(Value::is_boolean)
        }
        Some("counterOpponentStackObjects") => effect["bindCountAs"].is_string(),
        Some("suspendStackItem") => {
            target_reference_supported(&effect["object"])
                && integer_value(&effect["timeCounters"]).is_some_and(|count| count > 0)
        }
        Some("changeSingleTarget") => {
            target_reference_supported(&effect["object"])
                && player_reference_supported(&effect["player"])
        }
        Some("changeTargets") => {
            target_reference_supported(&effect["object"])
                && player_reference_supported(&effect["player"])
                && effect["optional"].is_boolean()
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_stack_casting_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_stack_casting_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_stack_casting_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("castAnyNumber") => {
                // The offered cards, fixed before any of them is cast.
                let offered = self
                    .resolve_runtime_object_ids(&effect["cards"], stack_object, bindings, decisions)
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                if let Some(binding) = effect["bind"].as_str() {
                    // Bound even when nothing is cast: a later `that weren't
                    // cast` subtracts an empty set, never a missing binding.
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(Vec::new()));
                }
                self.cast_bound_cards_without_paying(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
                if let Some(binding) = effect["bind"].as_str() {
                    // A cast card is the spell on the stack (CR 601.2a); the
                    // spells of this resolution keep their card identities.
                    let cast = self
                        .state
                        .stack
                        .iter()
                        .map(|object| object.card.instance_id.clone())
                        .filter(|instance_id| offered.contains(instance_id))
                        .collect::<Vec<_>>();
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(cast));
                }
            }
            Some("castParadigmCopy") => {
                self.cast_paradigm_copy(stack_object, provider)?;
            }
            Some("attachPermanent") => {
                let attachment_ids = match value_kind(&effect["attachment"]) {
                    Some("self" | "abilitySource") => {
                        vec![stack_object.card.instance_id.clone()]
                    }
                    _ => self
                        .resolve_runtime_object_ids(
                            &effect["attachment"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .or_else(|| {
                            self.resolve_target(&effect["attachment"], stack_object, bindings)
                                .and_then(|target| match target {
                                    TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                                    _ => None,
                                })
                        })
                        .unwrap_or_default(),
                };
                if attachment_ids.is_empty() {
                    return Ok(());
                }
                let instance_id = self
                    .resolve_runtime_object_ids(&effect["to"], stack_object, bindings, decisions)
                    .and_then(|objects| objects.into_iter().next())
                    .or_else(|| {
                        self.resolve_target(&effect["to"], stack_object, bindings)
                            .and_then(|target| match target {
                                TargetRef::Permanent { instance_id } => Some(instance_id),
                                _ => None,
                            })
                    });
                let Some(instance_id) = instance_id else {
                    return Ok(());
                };
                let Some((recipient_player, recipient_index)) =
                    self.permanent_position(&instance_id)
                else {
                    return Ok(());
                };
                let recipient_is_creature = self.permanent_is_creature(
                    &self.state.players[recipient_player].battlefield[recipient_index],
                );
                let recipient_is_land = self.permanent_has_card_type(
                    &self.state.players[recipient_player].battlefield[recipient_index],
                    "Land",
                );
                let mut attached_ids = Vec::new();
                for attachment_id in attachment_ids {
                    if let Some((player_index, card_index)) =
                        self.permanent_position(&attachment_id)
                    {
                        // CR 701.3b/701.3c: an object that cannot legally be
                        // attached to the recipient stays where it is, and an
                        // object never becomes attached to itself. Only an
                        // Aura, Equipment or Fortification can be attached; an
                        // Equipment only to a creature (CR 301.5c) and a
                        // Fortification only to a land (CR 301.6).
                        let type_line = &self.state.players[player_index].battlefield[card_index]
                            .definition
                            .type_line;
                        let attachable = type_line_contains(type_line, "Aura")
                            || (type_line_contains(type_line, "Equipment")
                                && recipient_is_creature)
                            || (type_line_contains(type_line, "Fortification")
                                && recipient_is_land);
                        if attachment_id == instance_id || !attachable {
                            continue;
                        }
                        self.state.players[player_index].battlefield[card_index].attached_to =
                            Some(instance_id.clone());
                        self.record_event(
                            "permanentAttached",
                            Some(stack_object.controller.clone()),
                            Some(attachment_id.clone()),
                            json!({ "attachedTo": instance_id }),
                        );
                        self.apply_stored_copy_aura(&attachment_id);
                        attached_ids.push(attachment_id);
                    }
                }
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(attached_ids));
                }
                self.refresh_control_effects();
            }
            Some("castOneCard") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let card_ids = self
                    .resolve_runtime_object_ids(&effect["cards"], stack_object, bindings, decisions)
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                let source_zone = effect["sourceZone"].as_str().unwrap_or("graveyard");
                let zone = match source_zone {
                    "graveyard" => &self.state.players[player_index].graveyard,
                    "hand" => &self.state.players[player_index].hand,
                    "exile" => &self.state.players[player_index].exile,
                    _ => return Ok(()),
                };
                let cards = zone
                    .iter()
                    .filter(|card| card_ids.contains(&card.instance_id))
                    .cloned()
                    .collect::<Vec<_>>();
                let mut options = self.cast_one_card_actions(
                    player_index,
                    &cards,
                    source_zone,
                    effect["normalCostsApply"].as_bool() == Some(true),
                    effect.get("spellWhere"),
                )?;
                // An absent hit or a card with no legal resulting spell
                // does not ask the player to decline an impossible cast.
                if options.is_empty() {
                    return Ok(());
                }
                if effect["optional"].as_bool() == Some(true) {
                    options.push(LegalAction {
                        id: format!("declineCastOne:{}", stack_object.id),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: "Do not cast a spell".to_string(),
                        card_instance_id: None,
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    });
                }
                if !options.is_empty() {
                    let action = self.choose_action(
                        provider,
                        EngineDecisionRequest {
                            id: format!("castOne:{}", stack_object.id),
                            kind: DecisionKind::ResolutionChoice,
                            player_id,
                            source_card: Some(stack_object.card.clone()),
                            source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                            choice: None,
                            options,
                        },
                    )?;
                    if action.kind == ActionKind::CastSpell {
                        let cast_card_id = action.card_instance_id.clone();
                        self.apply_priority_action(&action, provider)?;
                        if let (Some(binding_id), Some(cast_card_id)) =
                            (effect["bind"].as_str(), cast_card_id)
                        {
                            bindings.insert(
                                binding_id.to_string(),
                                RuntimeBinding::Objects(vec![cast_card_id]),
                            );
                        }
                    }
                }
            }
            Some("counterSpell") => {
                if let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["spell"], stack_object, bindings)
                {
                    let target = self
                        .state
                        .stack
                        .iter()
                        .find(|object| object.id == stack_id)
                        .cloned();
                    let exile_instead = effect["exileInstead"].as_bool() == Some(true)
                        || target.as_ref().is_some_and(|target| {
                            effect.get("exileInsteadWhere").is_some_and(|filter| {
                                matches_card_filter(&target.card.definition, filter)
                            })
                        });
                    self.counter_spell(
                        &stack_id,
                        &stack_object.card.instance_id,
                        &stack_object.controller,
                        exile_instead,
                    )?;
                    let countered = !self.state.stack.iter().any(|object| object.id == stack_id);
                    if countered && let Some(target) = target {
                        if exile_instead && let Some(binding_id) = effect["bindExiledAs"].as_str() {
                            bindings.insert(
                                binding_id.to_string(),
                                RuntimeBinding::Objects(vec![target.card.instance_id.clone()]),
                            );
                        }
                        if let Some(tokens) = effect.get("controllerCreatesTokens") {
                            let mut token_stack = stack_object.clone();
                            token_stack.controller = target.controller.clone();
                            self.create_tokens_with_provider(
                                &json!({
                                    "kind": "createTokens",
                                    "controller": { "kind": "abilityController" },
                                    "quantity": tokens["quantity"],
                                    "token": { "kind": "namedToken", "name": tokens["name"] },
                                }),
                                &token_stack,
                                bindings,
                                decisions,
                                provider,
                            )?;
                        }
                        if effect["prohibitControllerSpellsThisTurn"].as_bool() == Some(true) {
                            self.state.rule_modifiers.push(json!({
                                "kind": "prohibitAdditionalSpells",
                                "playerId": target.controller,
                                "expiresAfterTurn": self.state.turn_number,
                            }));
                        }
                        if effect["addManaAtNextMainPhase"].as_str() == Some("C") {
                            let amount = mana_value_with_decisions(
                                &target.card.definition,
                                &target.decisions,
                            );
                            let active_player_id =
                                self.state.players[self.state.active_player].id.as_str();
                            let before_first_main = active_player_id == stack_object.controller
                                && matches!(
                                    self.state.step,
                                    GameStep::Untap | GameStep::Upkeep | GameStep::Draw
                                );
                            let earliest_turn = if before_first_main {
                                self.state.turn_number
                            } else {
                                self.state.turn_number.saturating_add(1)
                            };
                            self.state.rule_modifiers.push(json!({
                                "kind": "plasmCaptureMana",
                                "playerId": stack_object.controller,
                                "amount": amount,
                                "earliestTurn": earliest_turn,
                                "sourceCard": stack_object.card,
                            }));
                        }
                    }
                }
            }
            Some("counterStackObject") => {
                if let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    self.counter_stack_object(
                        &stack_id,
                        &stack_object.card.instance_id,
                        &stack_object.controller,
                        effect["exileInstead"].as_bool().unwrap_or(false),
                    )?;
                }
            }
            Some("counterOpponentStackObjects") => {
                let stack_ids = self
                    .state
                    .stack
                    .iter()
                    .filter(|object| object.controller != stack_object.controller)
                    .map(|object| object.id.clone())
                    .collect::<Vec<_>>();
                let mut countered = 0;
                for stack_id in stack_ids {
                    let was_present = self.state.stack.iter().any(|object| object.id == stack_id);
                    self.counter_stack_object(
                        &stack_id,
                        &stack_object.card.instance_id,
                        &stack_object.controller,
                        false,
                    )?;
                    if was_present && !self.state.stack.iter().any(|object| object.id == stack_id) {
                        countered += 1;
                    }
                }
                if let Some(binding_id) = effect["bindCountAs"].as_str() {
                    bindings.insert(binding_id.to_string(), RuntimeBinding::Integer(countered));
                }
            }
            Some("suspendStackItem") => {
                if let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    let time_counters = integer_value(&effect["timeCounters"]).unwrap_or(1);
                    self.suspend_stack_object(&stack_id, time_counters)?;
                }
            }
            Some("changeSingleTarget") => {
                let Some(TargetRef::StackObject { stack_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((target_id, original_target)) = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == stack_id)
                    .and_then(|object| object.targets.iter().next())
                    .map(|(id, target)| (id.clone(), target.clone()))
                else {
                    return Ok(());
                };
                let candidates = match original_target {
                    TargetRef::Player { .. } => json!({ "kind": "players" }),
                    TargetRef::Permanent { .. } => {
                        json!({ "kind": "permanents", "where": Value::Null })
                    }
                    TargetRef::StackObject { .. } => {
                        json!({ "kind": "stackItems", "where": Value::Null })
                    }
                    TargetRef::Card { .. } => return Ok(()),
                };
                let Some(new_target) = self.choose_resolution_target(
                    "newTarget",
                    &candidates,
                    stack_object,
                    false,
                    provider,
                )?
                else {
                    return Ok(());
                };
                let affected_controller = self
                    .state
                    .stack
                    .iter()
                    .find(|object| object.id == stack_id)
                    .map(|object| object.controller.clone());
                if let Some(object) = self
                    .state
                    .stack
                    .iter_mut()
                    .find(|object| object.id == stack_id)
                {
                    object.targets.insert(target_id, new_target.clone());
                    self.record_event(
                        "stackObjectTargetChanged",
                        Some(stack_object.controller.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({
                            "stackId": stack_id,
                            "target": new_target,
                            "affectedControllerId": affected_controller,
                        }),
                    );
                }
            }
            Some("changeTargets") => {
                self.execute_target_reassignment(effect, stack_object, bindings, provider)?;
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
