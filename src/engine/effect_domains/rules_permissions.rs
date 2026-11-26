//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("installLifeForManaPermission") => {
            player_reference_supported(&effect["player"])
                && integer_value(&effect["life"]).is_some_and(|amount| amount > 0)
                && effect["symbol"]
                    .as_str()
                    .is_some_and(|symbol| matches!(symbol, "W" | "U" | "B" | "R" | "G" | "C"))
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("retainUnspentMana") => {
            player_reference_supported(&effect["player"])
                && effect["symbols"].as_array().is_some_and(|symbols| {
                    !symbols.is_empty()
                        && symbols.iter().all(|symbol| {
                            symbol.as_str().is_some_and(|symbol| {
                                matches!(symbol, "W" | "U" | "B" | "R" | "G" | "C")
                            })
                        })
                })
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("phaseOutPermanent") => {
            target_reference_supported(&effect["permanent"])
                || object_expression_supported(&effect["permanent"])
        }
        Some("createEmblemPermanentModifier") => {
            player_reference_supported(&effect["player"])
                && card_filter_supported(&effect["where"])
                && numeric_expression_supported(&effect["power"])
                && numeric_expression_supported(&effect["toughness"])
                && effect["keywords"].as_array().is_some_and(|keywords| {
                    keywords.iter().all(|keyword| {
                        keyword.as_str().is_some_and(|keyword| {
                            keyword_kind_supported(&json!({ "kind": keyword }))
                        })
                    })
                })
        }
        Some("createEmblem") => {
            player_reference_supported(&effect["player"])
                && effect["modifiers"].as_array().is_some_and(|modifiers| {
                    modifiers.iter().all(|modifier| {
                        matches!(
                            value_kind(modifier),
                            Some("noMaximumHandSize" | "maximumHandSize")
                        ) && battlefield_static_modifier_supported(modifier)
                    })
                })
                && effect["triggeredAbilities"]
                    .as_array()
                    .is_none_or(|abilities| abilities.iter().all(triggered_rule_supported))
                && (effect["modifiers"]
                    .as_array()
                    .is_some_and(|modifiers| !modifiers.is_empty())
                    || effect["triggeredAbilities"]
                        .as_array()
                        .is_some_and(|abilities| !abilities.is_empty()))
        }
        Some("ignoreLegendRuleUntilEndOfTurn") => player_reference_supported(&effect["player"]),
        Some("gainControlPermanent") => {
            (target_reference_supported(&effect["permanent"])
                || object_expression_supported(&effect["permanent"])
                // A relation-filtered collection (`all creatures blocking
                // it`, `all permanents they own`) resolved and prepared by
                // the shared runtime collection.
                || object_relation_eval::related_collection_supported(&effect["permanent"]))
                && player_reference_supported(&effect["controller"])
                && effect.get("duration").is_none_or(|duration| {
                    matches!(
                        value_kind(duration),
                        Some(
                            "untilEndOfCurrentTurn"
                                | "whileSourceControlled"
                                | "whileSourceTapped"
                                | "whileSourceOnBattlefield"
                        )
                    )
                })
        }
        Some("exchangeControl") => {
            value_kind(&effect["objects"]) == Some("chosenTargets")
                && effect["objects"]["id"].is_string()
        }

        Some("replaceGraveyardWithExileUntilEndOfTurn") => {
            player_reference_supported(&effect["player"])
        }
        Some("setPrepared") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && effect["value"].is_boolean()
        }
        Some("gainControlWhileSourceTapped") => {
            target_reference_supported(&effect["permanent"])
                && value_kind(&effect["source"]) == Some("self")
        }
        Some("setClassLevel") => {
            matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) && integer_value(&effect["value"]).is_some_and(|level| (2..=3).contains(&level))
        }
        Some("preventAttacksUntilNextTurn") => player_reference_supported(&effect["player"]),
        Some("preventPlayerDamageUntilNextTurn") => player_reference_supported(&effect["player"]),
        Some("suspendPermanent") => {
            target_reference_supported(&effect["permanent"])
                && integer_value(&effect["timeCounters"]).is_some_and(|count| count > 0)
        }
        Some("goadAllCreaturesUntilNextTurn") => true,
        // CR 701.38a over the shared object operand (a target or a set).
        Some("goadPermanents") => {
            object_expression_supported(&effect["objects"])
                || target_reference_supported(&effect["objects"])
        }
        // CR 701.35a (detain): one object's activated abilities, including
        // mana abilities, can't be activated until the named player's next turn.
        Some("prohibitPermanentActivations") => {
            target_reference_supported(&effect["object"])
                && (value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                    || (value_kind(&effect["duration"]) == Some("untilNextTurn")
                        && player_reference_supported(&effect["duration"]["player"])))
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_rules_permissions_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_rules_permissions_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_rules_permissions_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("setPrepared") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    self.set_permanent_prepared(
                        &instance_id,
                        effect["value"].as_bool().unwrap_or(true),
                    )?;
                } else if value_kind(&effect["duration"]) == Some("permanent")
                    && target_reference_supported(&effect["object"])
                {
                    let instance_id = self
                        .resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id }
                            | TargetRef::Card { instance_id } => Some(instance_id),
                            _ => None,
                        });
                    if let Some(instance_id) = instance_id
                        && let Some((player_index, card_index)) =
                            self.permanent_position(&instance_id)
                    {
                        let ability = effect["ability"].clone();
                        self.state.players[player_index].battlefield[card_index]
                            .definition
                            .printed_rules_mut()
                            .push(ability.clone());
                        self.record_event(
                            "abilityGranted",
                            Some(stack_object.controller.clone()),
                            Some(instance_id),
                            json!({ "ability": ability, "duration": effect["duration"] }),
                        );
                    }
                } else if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    self.set_permanent_prepared(
                        &stack_object.card.instance_id,
                        effect["value"].as_bool().unwrap_or(true),
                    )?;
                }
            }
            Some("retainUnspentMana") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "retainUnspentMana",
                        "playerId": player_id,
                        "symbols": effect["symbols"],
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("installLifeForManaPermission") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "lifeForManaPermission",
                    "playerId": player_id,
                    "life": effect["life"],
                    "symbol": effect["symbol"],
                    "expiresAfterTurn": self.state.turn_number,
                }));
            }
            Some("ignoreLegendRuleUntilEndOfTurn") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "ignoreLegendRule",
                        "playerId": player_id,
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("gainControlPermanent") => {
                let Some(controller_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let instance_ids = self
                    .resolve_runtime_object_ids(
                        &effect["permanent"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .or_else(|| {
                        self.resolve_target(&effect["permanent"], stack_object, bindings)
                            .and_then(|target| match target {
                                TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                                _ => None,
                            })
                    })
                    .unwrap_or_default();
                // CR 611.2b: `for as long as you control SOURCE` never starts
                // unless that same source object is controlled now.
                let while_source_controlled =
                    value_kind(&effect["duration"]) == Some("whileSourceControlled");
                if while_source_controlled
                    && !(self.linked_source_is_same_battlefield_object(&stack_object.card)
                        && self
                            .permanent_position(&stack_object.card.instance_id)
                            .is_some_and(|(player_index, card_index)| {
                                self.state.players[player_index].battlefield[card_index].controller
                                    == controller_id
                            }))
                {
                    return Ok(());
                }
                // CR 611.2b: a source-state window never starts unless the
                // same source object holds that state now.
                let source_state = value_kind(&effect["duration"]).filter(|kind| {
                    matches!(*kind, "whileSourceTapped" | "whileSourceOnBattlefield")
                });
                if let Some(window) = source_state
                    && !self.source_state_window_holds(stack_object, window)
                {
                    return Ok(());
                }
                for instance_id in instance_ids {
                    let original_controller =
                        self.permanent_position(&instance_id)
                            .map(|(player_index, card_index)| {
                                self.state.players[player_index].battlefield[card_index]
                                    .controller
                                    .clone()
                            });
                    if self
                        .change_permanent_controller(&instance_id, &controller_id)?
                        .is_some()
                    {
                        let temporary =
                            value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn");
                        if while_source_controlled
                            && let Some(original_controller) = original_controller.clone()
                            && original_controller != controller_id
                        {
                            self.state.rule_modifiers.push(json!({
                                "kind": "controlWhileSourceControlled",
                                "sourceCardInstanceId": stack_object.card.instance_id,
                                "sourceIdentity": Self::tracked_zone_identity(&stack_object.card),
                                "sourceController": controller_id,
                                "targetCardInstanceId": instance_id,
                                "originalController": original_controller,
                            }));
                        }
                        if let Some(window) = source_state
                            && let Some(original_controller) = original_controller.clone()
                            && original_controller != controller_id
                        {
                            self.record_source_state_control(
                                stack_object,
                                window,
                                &instance_id,
                                &original_controller,
                                &controller_id,
                            );
                        }
                        if temporary
                            && let Some(original_controller) = original_controller
                            && original_controller != controller_id
                        {
                            self.state.rule_modifiers.push(json!({
                                "kind": "controlUntilEndOfTurn",
                                "cardInstanceId": instance_id,
                                "originalController": original_controller,
                                "expiresAfterTurn": self.state.turn_number,
                            }));
                        }
                        self.record_event(
                            "controlChanged",
                            Some(controller_id.clone()),
                            Some(instance_id),
                            json!({ "duration": if temporary { "untilEndOfTurn" } else { "indefinite" } }),
                        );
                    }
                }
            }
            Some("exchangeControl") => {
                let object_ids = self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                if object_ids.len() != 2 {
                    return Ok(());
                }
                let controllers = object_ids
                    .iter()
                    .map(|instance_id| {
                        self.permanent_position(instance_id)
                            .map(|(player_index, card_index)| {
                                self.state.players[player_index].battlefield[card_index]
                                    .controller
                                    .clone()
                            })
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(controllers) = controllers else {
                    return Ok(());
                };
                self.change_permanent_controller(&object_ids[0], &controllers[1])?;
                self.change_permanent_controller(&object_ids[1], &controllers[0])?;
                self.record_event(
                    "controlExchanged",
                    Some(stack_object.controller.clone()),
                    Some(object_ids[0].clone()),
                    json!({ "otherPermanentId": object_ids[1] }),
                );
            }
            Some("phaseOutPermanent") => {
                let mut instance_ids = self
                    .resolve_runtime_object_ids(
                        &effect["permanent"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                if instance_ids.is_empty()
                    && let Some(TargetRef::Permanent { instance_id }) =
                        self.resolve_target(&effect["permanent"], stack_object, bindings)
                {
                    instance_ids.push(instance_id);
                }
                for instance_id in instance_ids {
                    self.phase_out_permanent(&instance_id);
                }
            }
            Some("preventAttacksUntilNextTurn") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "creaturesCantAttackPlayer",
                        "playerId": player_id,
                        "expiresAtTurnStart": stack_object.controller,
                    }));
                }
            }
            Some("preventPlayerDamageUntilNextTurn") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "preventPlayerDamage",
                        "playerId": player_id,
                        "expiresAtTurnStart": stack_object.controller,
                        "installedTurn": self.state.turn_number,
                    }));
                }
            }

            Some("suspendPermanent") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.exile_permanent(&instance_id)?;
                if let Some(card) = self
                    .state
                    .players
                    .iter_mut()
                    .flat_map(|player| player.exile.iter_mut())
                    .find(|card| card.instance_id == instance_id)
                {
                    card.flags.insert("suspended".to_string(), true);
                    card.counters.insert(
                        "time".to_string(),
                        integer_value(&effect["timeCounters"]).unwrap_or(1),
                    );
                }
            }

            Some("gainControlWhileSourceTapped") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                    && let Some((target_player, target_index)) =
                        self.permanent_position(&instance_id)
                {
                    let original_controller = self.state.players[target_player].battlefield
                        [target_index]
                        .controller
                        .clone();
                    self.change_permanent_controller(&instance_id, &stack_object.controller)?;
                    self.state.rule_modifiers.push(json!({
                        "kind": "controlWhileSourceTapped",
                        "sourceCardInstanceId": stack_object.card.instance_id,
                        "sourceController": stack_object.controller,
                        "targetCardInstanceId": instance_id,
                        "originalController": original_controller,
                    }));
                    self.record_event(
                        "controlChanged",
                        Some(stack_object.controller.clone()),
                        Some(instance_id),
                        json!({
                            "sourceCardInstanceId": stack_object.card.instance_id,
                            "untilSourceUntapped": true,
                        }),
                    );
                }
            }
            Some("setClassLevel") => {
                let level = integer_value(&effect["value"]).unwrap_or(1);
                if let Some((player_index, card_index)) =
                    self.permanent_position(&stack_object.card.instance_id)
                {
                    self.state.players[player_index].battlefield[card_index]
                        .counters
                        .insert("classLevel".to_string(), level);
                    let source = self.state.players[player_index].battlefield[card_index].clone();
                    self.record_event(
                        "classLeveled",
                        Some(source.controller.clone()),
                        Some(source.instance_id.clone()),
                        json!({ "level": level }),
                    );
                    self.enqueue_class_level_triggers(&source, level);
                }
            }
            Some("createEmblemPermanentModifier") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let mut where_filter = effect["where"].clone();
                if value_kind(&where_filter) == Some("chosenCreatureType")
                    && let Some(chosen_type) = where_filter["decisionId"]
                        .as_str()
                        .and_then(|decision_id| decisions.get(decision_id))
                        .and_then(|value| {
                            value.as_str().or_else(|| {
                                value
                                    .as_array()
                                    .and_then(|values| values.first())
                                    .and_then(Value::as_str)
                            })
                        })
                {
                    where_filter = json!({
                        "kind": "subtypeContains",
                        "value": chosen_type,
                    });
                }
                let emblem = json!({
                    "kind": "emblemPermanentModifier",
                    "playerId": player_id,
                    "where": where_filter,
                    "power": effect["power"].clone(),
                    "toughness": effect["toughness"].clone(),
                    "keywords": effect["keywords"].clone(),
                    "sourceCardInstanceId": stack_object.card.instance_id,
                });
                self.state.rule_modifiers.push(emblem.clone());
                self.record_event(
                    "emblemCreated",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    emblem,
                );
            }
            Some("createEmblem") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let mut created = Vec::new();
                for modifier in effect["modifiers"].as_array().into_iter().flatten() {
                    let runtime_modifier = match value_kind(modifier) {
                        Some("noMaximumHandSize") => json!({
                            "kind": "noMaximumHandSize",
                            "playerId": player_id,
                            "sourceCardInstanceId": stack_object.card.instance_id,
                        }),
                        Some("maximumHandSize") => json!({
                            "kind": "maximumHandSize",
                            "playerId": player_id,
                            "amount": modifier["amount"],
                            "sourceCardInstanceId": stack_object.card.instance_id,
                        }),
                        _ => continue,
                    };
                    self.state.rule_modifiers.push(runtime_modifier.clone());
                    created.push(runtime_modifier);
                }
                for ability in effect["triggeredAbilities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let runtime_modifier = json!({
                        "kind": "emblemTriggeredAbility",
                        "playerId": player_id,
                        "sourceCard": stack_object.card,
                        "ability": ability,
                    });
                    self.state.rule_modifiers.push(runtime_modifier.clone());
                    created.push(runtime_modifier);
                }
                self.record_event(
                    "emblemCreated",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "modifiers": created }),
                );
            }
            Some("replaceGraveyardWithExileUntilEndOfTurn") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "temporaryGraveyardDestinationReplacement",
                    "playerId": player_id,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                    "expiresAfterTurn": self.state.turn_number,
                }));
            }
            Some("goadPermanents") => {
                // CR 701.38a: the ability controller goads each creature;
                // a target that became illegal or is no creature is ignored.
                let instance_ids = if object_expression_supported(&effect["objects"]) {
                    self.resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default()
                } else {
                    self.resolve_target(&effect["objects"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                            _ => None,
                        })
                        .unwrap_or_default()
                };
                let mut goaded = BTreeSet::new();
                for instance_id in instance_ids {
                    let is_creature =
                        self.permanent_position(&instance_id)
                            .is_some_and(|(player, index)| {
                                self.permanent_is_creature(
                                    &self.state.players[player].battlefield[index],
                                )
                            });
                    if is_creature && goaded.insert(instance_id.clone()) {
                        self.goad_permanent(&instance_id, &stack_object.controller);
                    }
                }
            }
            Some("prohibitPermanentActivations") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let mut modifier = json!({
                    "kind": "temporaryActivationProhibition",
                    "cardInstanceId": instance_id,
                });
                if value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn") {
                    // `this turn`: the general end-of-turn cleanup removes it.
                    modifier["expiresAfterTurn"] = json!(self.state.turn_number);
                } else {
                    let Some(expiry_player_id) =
                        self.resolve_player(&effect["duration"]["player"], stack_object, bindings)
                    else {
                        return Ok(());
                    };
                    modifier["expiresAtTurnStart"] = json!(expiry_player_id);
                }
                if self.permanent_position(&instance_id).is_some() {
                    self.state.rule_modifiers.push(modifier);
                }
            }
            Some("goadAllCreaturesUntilNextTurn") => {
                let creature_ids = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| self.permanent_is_creature(permanent))
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                for instance_id in creature_ids {
                    self.goad_permanent(&instance_id, &stack_object.controller);
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
