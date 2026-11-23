//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("sacrificeAtNextEndStep") => target_reference_supported(&effect["object"]),

        Some("createReflexiveTrigger") => {
            value_kind(&effect["source"]) == Some("self")
                && player_reference_supported(&effect["controller"])
                && (effect.get("ability").is_some_and(triggered_rule_supported)
                    || effect["effects"].as_array().is_some_and(|effects| {
                        !effects.is_empty() && effects.iter().all(effect_supported)
                    }))
        }
        Some("installAttackTrigger") => {
            player_reference_supported(&effect["player"])
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("untilEndOfCurrentTurn" | "untilNextTurn")
                )
                && (value_kind(&effect["duration"]) != Some("untilNextTurn")
                    || player_reference_supported(&effect["duration"]["player"]))
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
                && effect.get("declaration").is_none_or(|declaration| {
                    declaration_shape_supported(&json!({ "declaration": declaration }))
                })
        }
        Some("installOpponentAttackTrigger") => {
            player_reference_supported(&effect["controller"])
                && value_kind(&effect["duration"]) == Some("untilNextTurn")
                && player_reference_supported(&effect["duration"]["player"])
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        Some("installDelayedTriggeredAbility") => {
            player_reference_supported(&effect["controller"])
                && (permanent_attacks_event_supported(&effect["event"])
                    || (value_kind(&effect["event"])
                        == Some("permanentAttacksProtectedPlayerOrPlaneswalker")
                        && player_reference_supported(&effect["event"]["protectedPlayer"])))
                && value_kind(&effect["duration"]) == Some("untilNextTurn")
                && player_reference_supported(&effect["duration"]["player"])
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        Some("installCombatDamageTrigger") => {
            (target_reference_supported(&effect["object"])
                || matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ))
                && value_kind(&effect["duration"]) == Some("currentCombat")
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        Some("resolveOrdinalTriggeredAbility") => {
            effect["id"].is_string()
                && effect["branches"].as_array().is_some_and(|branches| {
                    !branches.is_empty()
                        && branches.iter().all(|branch| {
                            integer_value(&branch["ordinal"]).is_some_and(|ordinal| ordinal > 0)
                                && branch["effects"].as_array().is_some_and(|effects| {
                                    !effects.is_empty() && effects.iter().all(effect_supported)
                                })
                        })
                })
        }
        Some("scheduleSacrificeAtEndCombat") => target_reference_supported(&effect["object"]),
        Some("installDelayedSpellCastTrigger") => {
            player_reference_supported(&effect["player"])
                && card_filter_supported(&effect["where"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        Some("installDelayedStepTrigger") => {
            matches!(
                effect["step"].as_str(),
                Some("upkeep" | "endStep" | "endOfCombat")
            ) && player_reference_supported(&effect["controller"])
                && effect.get("captureBindings").is_none_or(|captured| {
                    captured.as_array().is_some_and(|captured| {
                        !captured.is_empty()
                            && captured.iter().all(Value::is_string)
                            && captured
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<BTreeSet<_>>()
                                .len()
                                == captured.len()
                    })
                })
                && effect
                    .get("trackedObject")
                    .is_none_or(target_reference_supported)
                && effect.get("turnOf").is_none_or(player_reference_supported)
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_triggers_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_triggers_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_triggers_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("installAttackTrigger") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let mut modifier = json!({
                    "kind": "delayedAttackTrigger",
                    "playerId": player_id,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                    "sourceCard": stack_object.card,
                    "effects": effect["effects"],
                    "expiresAfterTurn": self.state.turn_number,
                });
                if let Some(declaration) = effect.get("declaration") {
                    modifier["declaration"] = declaration.clone();
                }
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "delayedAttackTriggerInstalled",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("installOpponentAttackTrigger") => {
                let Some(controller_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let modifier = json!({
                    "kind": "delayedOpponentAttackTrigger",
                    "controllerId": controller_id,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                    "sourceCard": stack_object.card,
                    "effects": effect["effects"],
                    "expiresAtTurnStart": controller_id,
                    "installedTurn": self.state.turn_number,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "delayedOpponentAttackTriggerInstalled",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("installDelayedTriggeredAbility") => {
                let Some(controller_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(expires_at_turn_start) =
                    self.resolve_player(&effect["duration"]["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let event = if value_kind(&effect["event"]) == Some("permanentAttacks") {
                    let Some(defender) = self.materialize_attack_defender_predicate(
                        &effect["event"]["defender"],
                        stack_object,
                        bindings,
                    ) else {
                        return Ok(());
                    };
                    json!({
                        "kind": "permanentAttacks",
                        "attacker": effect["event"]["attacker"].clone(),
                        "defender": defender,
                    })
                } else {
                    let Some(protected_player_id) = self.resolve_player(
                        &effect["event"]["protectedPlayer"],
                        stack_object,
                        bindings,
                    ) else {
                        return Ok(());
                    };
                    json!({
                        "kind": "permanentAttacksProtectedPlayerOrPlaneswalker",
                        "protectedPlayerId": protected_player_id,
                    })
                };
                let modifier = json!({
                    "kind": "delayedTriggeredAbility",
                    "controllerId": controller_id,
                    "event": event,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                    "sourceCard": stack_object.card,
                    "effects": effect["effects"],
                    "expiresAtTurnStart": expires_at_turn_start,
                    "installedTurn": self.state.turn_number,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "delayedTriggeredAbilityInstalled",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("installCombatDamageTrigger") => {
                let instance_id = if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    stack_object.card.instance_id.clone()
                } else if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    instance_id
                } else {
                    return Ok(());
                };
                let modifier = json!({
                    "kind": "delayedCombatDamageTrigger",
                    "permanentId": instance_id,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                    "sourceCard": stack_object.card,
                    "effects": effect["effects"],
                    "expiresAfterCombatTurn": self.state.turn_number,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "delayedCombatDamageTriggerInstalled",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("createReflexiveTrigger") => {
                let Some(player_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let (ability, reflexive_decisions, reflexive_targets) = if let Some(ability) =
                    effect.get("ability")
                {
                    let mut ability = ability.clone();
                    ability = self.materialize_reflexive_numeric_expressions(
                        &ability,
                        stack_object,
                        bindings,
                        decisions,
                    );
                    ability = self.materialize_runtime_object_bindings(&ability, bindings);
                    ability = self.materialize_runtime_target_bindings(&ability, bindings);
                    let inherited_targets = bindings
                        .iter()
                        .filter_map(|(id, binding)| match binding {
                            RuntimeBinding::Target(target) => Some((id.clone(), target.clone())),
                            _ => None,
                        })
                        .collect::<BTreeMap<_, _>>();
                    let inherited_decisions = bindings
                        .iter()
                        .filter_map(|(id, binding)| match binding {
                            RuntimeBinding::Objects(objects) => Some((
                                id.clone(),
                                Value::Array(
                                    objects
                                        .iter()
                                        .cloned()
                                        .map(Value::String)
                                        .collect::<Vec<_>>(),
                                ),
                            )),
                            _ => None,
                        })
                        .collect::<BTreeMap<_, _>>();
                    if ability.get("condition").is_some_and(|condition| {
                        !self.runtime_condition_value(
                            condition,
                            stack_object,
                            bindings,
                            &inherited_decisions,
                        )
                    }) {
                        return Ok(());
                    }
                    // CR 603.3d removes a reflexive ability only when it can't
                    // choose its targets; one that declares nothing goes on the
                    // stack with the bindings it inherits.
                    if ability["declaration"]["decisions"]
                        .as_array()
                        .is_none_or(Vec::is_empty)
                    {
                        (ability, inherited_decisions, inherited_targets)
                    } else {
                        let declarations = self
                            .declarations_for_triggered_rule(
                                &ability,
                                &player_id,
                                &stack_object.card,
                                &stack_object.decisions,
                            )?
                            .unwrap_or_default();
                        if declarations.is_empty() {
                            return Ok(());
                        }
                        let options = declarations
                            .into_iter()
                            .enumerate()
                            .map(|(index, declaration)| LegalAction {
                                id: format!("reflexiveTargets:{}:{index}", stack_object.id),
                                kind: ActionKind::ChooseResolution,
                                player_id: player_id.clone(),
                                label: "Choose targets for the reflexive ability".to_string(),
                                card_instance_id: Some(stack_object.card.instance_id.clone()),
                                payment_sources: Vec::new(),
                                decisions: declaration.decisions,
                                targets: declaration.targets,
                                target_order: declaration.target_order,
                                attacker_id: None,
                                blocker_id: None,
                            })
                            .collect::<Vec<_>>();
                        let choice = self.choose_action(
                            provider,
                            EngineDecisionRequest {
                                id: format!("reflexiveTargets:{}", stack_object.id),
                                kind: DecisionKind::ResolutionChoice,
                                player_id: player_id.clone(),
                                source_card: Some(stack_object.card.clone()),
                                source_card_instance_id: Some(
                                    stack_object.card.instance_id.clone(),
                                ),
                                choice: None,
                                options,
                            },
                        )?;
                        let mut reflexive_decisions = inherited_decisions;
                        reflexive_decisions.extend(choice.decisions);
                        let mut reflexive_targets = inherited_targets;
                        reflexive_targets.extend(choice.targets);
                        (ability, reflexive_decisions, reflexive_targets)
                    }
                } else {
                    let effects = self.materialize_reflexive_numeric_expressions(
                        &effect["effects"],
                        stack_object,
                        bindings,
                        decisions,
                    );
                    (
                        json!({
                            "kind": "triggeredAbility",
                            "source": { "kind": "self" },
                            "effects": effects,
                        }),
                        BTreeMap::new(),
                        BTreeMap::new(),
                    )
                };
                self.stack_sequence += 1;
                let reflexive_stack_id = format!("stack:{}", self.stack_sequence);
                self.state.stack.push(StackObject {
                    id: reflexive_stack_id.clone(),
                    controller: player_id.clone(),
                    card: stack_object.card.clone(),
                    cant_be_countered: false,
                    exile_on_leave_stack: false,
                    ability_kind: Some("triggeredAbility".to_string()),
                    ability_rule: Some(ability),
                    decisions: reflexive_decisions,
                    targets: reflexive_targets,
                });
                self.record_event(
                    "reflexiveTriggeredAbilityPutOnStack",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({
                        "cardName": stack_object.card.definition.name,
                        "objectKind": "triggeredAbility",
                        "stackId": reflexive_stack_id,
                    }),
                );
            }
            Some("scheduleSacrificeAtEndCombat") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "sacrificeAtEndCombat",
                    "cardInstanceId": instance_id,
                }));
            }
            Some("sacrificeAtNextEndStep") => {
                if let Some(instance_id) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id }
                            | TargetRef::Card { instance_id } => Some(instance_id),
                            _ => None,
                        })
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "sacrificeAtEndStep",
                        "cardInstanceId": instance_id,
                        "notBeforeTurn": self.state.turn_number,
                    }));
                }
            }

            Some("resolveOrdinalTriggeredAbility") => {
                let id = effect["id"].as_str().unwrap_or("ordinalAbility");
                let event_kind = format!("ordinalAbilityResolved:{id}");
                let previous = self
                    .state
                    .events
                    .iter()
                    .filter(|event| {
                        event.turn_number == self.state.turn_number
                            && event.kind == event_kind
                            && event.card_instance_id.as_deref()
                                == Some(stack_object.card.instance_id.as_str())
                    })
                    .count();
                let ordinal = previous.saturating_add(1) as i32;
                self.record_event(
                    &event_kind,
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "ordinal": ordinal }),
                );
                if let Some(branch) = effect["branches"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|branch| integer_value(&branch["ordinal"]) == Some(ordinal))
                {
                    self.execute_effects(
                        &branch["effects"],
                        stack_object,
                        bindings,
                        decisions,
                        provider,
                    )?;
                }
            }

            Some("installDelayedSpellCastTrigger") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let modifier = json!({
                    "kind": "delayedSpellCastTrigger",
                    "playerId": player_id,
                    "where": effect["where"].clone(),
                    "effects": effect["effects"].clone(),
                    "sourceCard": stack_object.card.clone(),
                    "expiresAfterTurn": self.state.turn_number,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "delayedTriggerInstalled",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("installDelayedStepTrigger") => {
                let Some(controller_id) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let delayed_effects = self.materialize_runtime_numeric_expressions(
                    &effect["effects"],
                    stack_object,
                    bindings,
                    decisions,
                );
                let delayed_effects =
                    self.materialize_runtime_object_bindings(&delayed_effects, bindings);
                let mut captured_bindings = serde_json::Map::new();
                for binding_id in effect["captureBindings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    let captured_value = match bindings.get(binding_id) {
                        Some(RuntimeBinding::Player(player_id))
                        | Some(RuntimeBinding::Target(TargetRef::Player { player_id })) => {
                            Value::String(player_id.clone())
                        }
                        Some(RuntimeBinding::Objects(instance_ids)) => {
                            Value::Array(instance_ids.iter().cloned().map(Value::String).collect())
                        }
                        _ => {
                            return Err(EngineError::new(format!(
                                "delayed trigger capture {binding_id} is not a capturable binding"
                            )));
                        }
                    };
                    // An object binding travels with the identities of the
                    // objects it named (CR 603.7c, 400.7).
                    if let Some(RuntimeBinding::Objects(instance_ids)) = bindings.get(binding_id) {
                        captured_bindings.insert(
                            super::super::captured_binding::captured_identities_key(binding_id),
                            self.captured_object_identities(instance_ids),
                        );
                    }
                    captured_bindings.insert(binding_id.to_string(), captured_value);
                }
                // An event reference bound when the ability resolved (the
                // other combatant) travels with its object identity.
                for (key, value) in
                    self.transported_event_references(&effect["effects"], stack_object)
                {
                    captured_bindings.insert(key, value);
                }
                let tracked_object_id = if let Some(object) = effect.get("trackedObject") {
                    let Some(instance_id) = self
                        .resolve_target(object, stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id }
                            | TargetRef::Card { instance_id } => Some(instance_id),
                            _ => None,
                        })
                    else {
                        return Ok(());
                    };
                    Some(instance_id)
                } else {
                    None
                };
                // `your next STEP` (CR 603.7): only that player's step wakes it.
                let turn_player_id = match effect.get("turnOf") {
                    Some(player) => {
                        let Some(player_id) = self.resolve_player(player, stack_object, bindings)
                        else {
                            return Ok(());
                        };
                        Some(player_id)
                    }
                    None => None,
                };
                let modifier = json!({
                    "kind": "delayedStepTrigger",
                    "step": effect["step"],
                    "turnPlayerId": turn_player_id,
                    "controllerId": controller_id,
                    "effects": delayed_effects,
                    "sourceCard": stack_object.card,
                    "trackedObjectId": tracked_object_id,
                    "capturedBindings": captured_bindings,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "delayedTriggerInstalled",
                    Some(controller_id),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
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
