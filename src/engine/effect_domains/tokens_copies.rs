//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("conjureNamedCard") => {
            player_reference_supported(&effect["player"])
                && effect["name"].is_string()
                && matches!(effect["destination"].as_str(), Some("hand" | "battlefield"))
        }
        Some("createTokens") => {
            let token_supported = (numeric_expression_supported(&effect["token"]["power"])
                && numeric_expression_supported(&effect["token"]["toughness"]))
                || (value_kind(&effect["token"]) == Some("namedToken")
                    && effect["token"]["name"].as_str().is_some_and(|name| {
                        named_token_rules(name).is_some()
                            || matches!(name, "Clue" | "Food" | "Treasure")
                    }));
            numeric_expression_supported(&effect["quantity"])
                && token_supported
                && effect
                    .get("controller")
                    .is_none_or(player_reference_supported)
                && effect["token"]["abilities"]
                    .as_array()
                    .is_none_or(|abilities| {
                        abilities.iter().all(|ability| {
                            rule_supported(ability)
                                || keyword_rule_supported(ability)
                                || keyword_kind_supported(ability)
                        })
                    })
                && effect["counters"].as_array().is_none_or(|counters| {
                    counters.iter().all(|counter| {
                        counter["counter"].is_string()
                            && numeric_expression_supported(&counter["count"])
                    })
                })
                && effect.get("tapped").is_none_or(Value::is_boolean)
                // CR 205.4: supertypes named by the effect precede the types.
                && effect["token"].get("supertypes").is_none_or(|supertypes| {
                    supertypes.as_array().is_some_and(|supertypes| {
                        supertypes.iter().all(|supertype| {
                            matches!(supertype.as_str(), Some("Legendary" | "Snow" | "Basic"))
                        })
                    })
                })
                && effect
                    .get("exileWhenSourceLeaves")
                    .is_none_or(Value::is_boolean)
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("becomeCopyOfPermanent") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && target_reference_supported(&effect["copy"])
                && effect["retainResolvingAbility"].is_boolean()
                && effect.get("duration").is_none_or(|duration| {
                    matches!(
                        value_kind(duration),
                        Some("untilEndOfCurrentTurn" | "whileSourceOnBattlefield")
                    )
                })
        }
        Some("createCardCopy") => {
            target_reference_supported(&effect["card"])
                && effect["fromZone"].as_str() == Some("exile")
                && player_reference_supported(&effect["player"])
                && effect["to"].as_str() == Some("exile")
                && effect["bind"].is_string()
        }
        Some("ceaseToExist") => {
            object_expression_supported(&effect["objects"])
                && effect["fromZone"].as_str() == Some("exile")
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_tokens_copies_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_tokens_copies_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_tokens_copies_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("createCardCopy") => {
                let Some(TargetRef::Card { instance_id }) =
                    self.resolve_target(&effect["card"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(original) = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.exile.iter())
                    .find(|card| card.instance_id == instance_id)
                    .cloned()
                else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                self.copy_sequence += 1;
                let copy_id = format!("copy:card:{}:{}", original.instance_id, self.copy_sequence);
                let mut copy = CardInstance {
                    instance_id: copy_id.clone(),
                    definition: original.definition,
                    printed_definition: None,
                    owner: player_id.clone(),
                    controller: player_id.clone(),
                    tapped: false,
                    summoning_sick: false,
                    damage_marked: 0,
                    power_modifier: 0,
                    toughness_modifier: 0,
                    counters: BTreeMap::new(),
                    flags: BTreeMap::from([("isSpellCopy".to_string(), true)]),
                    battle_protector: None,
                    attached_to: None,
                };
                self.admit_new_card_carrier(&mut copy)?;
                self.state.players[player_index].exile.push(copy);
                if let Some(binding_id) = effect["bind"].as_str() {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Objects(vec![copy_id.clone()]),
                    );
                }
                self.record_event(
                    "cardCopyCreated",
                    Some(player_id),
                    Some(copy_id.clone()),
                    json!({
                        "copiedCardInstanceId": instance_id,
                        "sourceCardInstanceId": stack_object.card.instance_id,
                        "zone": "exile",
                    }),
                );
            }
            Some("ceaseToExist") => {
                let copy_ids = self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                for copy_id in copy_ids {
                    let player_index = self.state.players.iter().position(|player| {
                        player
                            .exile
                            .iter()
                            .any(|card| card.instance_id == copy_id && is_spell_copy(card))
                    });
                    if let Some(player_index) = player_index {
                        self.remove_ephemeral_copy_from_exile(
                            player_index,
                            &copy_id,
                            &stack_object.card.instance_id,
                        );
                    }
                }
            }
            Some("conjureNamedCard") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(name) = effect["name"].as_str() else {
                    return Ok(());
                };
                let Some(definition) =
                    compile_named_card_definition(name).map_err(EngineError::new)?
                else {
                    return Err(EngineError::new(format!(
                        "conjured card is absent from the catalog: {name}"
                    )));
                };
                self.copy_sequence += 1;
                let instance_id = format!(
                    "{}:conjured:{}:{}",
                    player_id, self.copy_sequence, definition.id
                );
                let mut card = CardInstance {
                    instance_id: instance_id.clone(),
                    printed_definition: None,
                    definition,
                    owner: player_id.clone(),
                    controller: player_id.clone(),
                    tapped: false,
                    summoning_sick: true,
                    damage_marked: 0,
                    power_modifier: 0,
                    toughness_modifier: 0,
                    counters: BTreeMap::new(),
                    flags: BTreeMap::from([("conjured".to_string(), true)]),
                    battle_protector: None,
                    attached_to: None,
                };
                let player_index = self.player_index(&player_id)?;
                match effect["destination"].as_str() {
                    Some("battlefield") => {
                        self.apply_enter_replacements(&mut card, provider)?;
                        self.admit_new_card_carrier(&mut card)?;
                        let trigger_source = card.clone();
                        self.state.players[player_index].battlefield.push(card);
                        self.enqueue_enter_battlefield_triggers(&trigger_source);
                    }
                    Some("library") => {
                        card.summoning_sick = false;
                        self.admit_new_card_carrier(&mut card)?;
                        self.state.players[player_index].library.push(card);
                    }
                    _ => {
                        card.summoning_sick = false;
                        self.admit_new_card_carrier(&mut card)?;
                        self.state.players[player_index].hand.push(card);
                    }
                }
                self.record_event(
                    "cardConjured",
                    Some(player_id),
                    Some(instance_id),
                    json!({ "name": name, "destination": effect["destination"] }),
                );
                self.enqueue_conjured_triggers(&stack_object.controller);
            }
            Some("becomeCopyOfPermanent") => {
                let Some(copy_id) =
                    self.resolve_target(&effect["copy"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id }
                            | TargetRef::Card { instance_id } => Some(instance_id),
                            TargetRef::Player { .. } | TargetRef::StackObject { .. } => None,
                        })
                else {
                    return Ok(());
                };
                let copied_definition = self
                    .state
                    .rule_modifiers
                    .iter()
                    .find(|modifier| {
                        value_kind(modifier) == Some("exiledWithSource")
                            && modifier["sourceCardInstanceId"].as_str()
                                == Some(stack_object.card.instance_id.as_str())
                            && modifier["cardInstanceId"].as_str() == Some(copy_id.as_str())
                    })
                    .and_then(|modifier| {
                        serde_json::from_value::<CardDefinition>(
                            modifier["lastKnownDefinition"].clone(),
                        )
                        .ok()
                    })
                    .or_else(|| {
                        let (player_index, card_index) = self.permanent_position(&copy_id)?;
                        Some(
                            self.state.players[player_index].battlefield[card_index]
                                .definition
                                .clone(),
                        )
                    });
                let Some(copied_definition) = copied_definition else {
                    return Ok(());
                };
                let source_id = stack_object.card.instance_id.clone();
                let object_id = if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    source_id.clone()
                } else if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    instance_id
                } else {
                    return Ok(());
                };
                let Some((object_player_index, object_card_index)) =
                    self.permanent_position(&object_id)
                else {
                    return Ok(());
                };
                let original_definition = self.state.players[object_player_index].battlefield
                    [object_card_index]
                    .definition
                    .clone();
                let original_printed_definition = self.state.players[object_player_index]
                    .battlefield[object_card_index]
                    .printed_definition
                    .clone();
                let retained_rule = effect["retainResolvingAbility"]
                    .as_bool()
                    .unwrap_or(false)
                    .then(|| stack_object.ability_rule.clone())
                    .flatten()
                    .or_else(|| {
                        original_definition
                            .rules
                            .iter()
                            .find(|rule| {
                                value_kind(rule) == Some("activatedAbility")
                                    && rule["effects"].as_array().is_some_and(|effects| {
                                        effects.iter().any(|effect| {
                                            value_kind(effect) == Some("becomeCopyOfPermanent")
                                        })
                                    })
                            })
                            .cloned()
                    });
                let mut definition = copied_definition;
                definition.is_commander = original_definition.is_commander;
                definition.is_game_piece = original_definition.is_game_piece;
                definition.is_sideboard = original_definition.is_sideboard;
                definition.is_token = original_definition.is_token;
                if let Some(rule) = retained_rule
                    && !definition.rules.contains(&rule)
                {
                    definition.rules.push(rule);
                }
                let temporary = value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn");
                if temporary {
                    self.state.players[object_player_index].battlefield[object_card_index]
                        .printed_definition = Some(original_definition.clone());
                    self.state.players[object_player_index].battlefield[object_card_index]
                        .flags
                        .insert("temporaryCopyUntilEndOfTurn".to_string(), true);
                    self.state.rule_modifiers.retain(|modifier| {
                        value_kind(modifier) != Some("temporaryCopyUntilEndOfTurn")
                            || modifier["cardInstanceId"].as_str() != Some(object_id.as_str())
                    });
                    self.state.rule_modifiers.push(json!({
                        "kind": "temporaryCopyUntilEndOfTurn",
                        "cardInstanceId": object_id.clone(),
                        "originalDefinition": original_definition.clone(),
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                }
                if value_kind(&effect["duration"]) == Some("whileSourceOnBattlefield") {
                    self.state.players[object_player_index].battlefield[object_card_index]
                        .printed_definition = Some(original_definition.clone());
                    self.state.rule_modifiers.retain(|modifier| {
                        value_kind(modifier) != Some("copyWhileSourceOnBattlefield")
                            || modifier["targetCardInstanceId"].as_str() != Some(object_id.as_str())
                    });
                    self.state.rule_modifiers.push(json!({
                        "kind": "copyWhileSourceOnBattlefield",
                        "sourceCardInstanceId": source_id,
                        "targetCardInstanceId": object_id.clone(),
                        "originalDefinition": original_definition,
                        "originalPrintedDefinition": original_printed_definition,
                    }));
                }
                self.state.players[object_player_index].battlefield[object_card_index].definition =
                    definition;
                self.record_event(
                    "permanentBecameCopy",
                    Some(stack_object.controller.clone()),
                    Some(object_id),
                    json!({ "copiedPermanentId": copy_id, "retainedResolvingAbility": effect["retainResolvingAbility"] }),
                );
            }
            Some("createTokens") => {
                let created_ids = self.create_tokens_with_provider(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
                if effect["exileWhenSourceLeaves"].as_bool() == Some(true) {
                    for token_id in &created_ids {
                        self.state.rule_modifiers.push(json!({
                            "kind": "exileTokenWhenSourceLeaves",
                            "sourceCardInstanceId": stack_object.card.instance_id,
                            "tokenId": token_id,
                        }));
                    }
                }
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(created_ids));
                }
            }
            Some("createTokenCopy") => {
                self.create_token_copy(effect, stack_object, provider)?;
            }
            Some("createTokenCopyOfPermanent") => {
                self.create_token_copy_of_permanent(effect, stack_object, bindings, provider)?;
            }
            Some("createModifiedTokenCopy") => {
                self.create_modified_token_copy(effect, stack_object, bindings, provider)?;
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
