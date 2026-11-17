//! Structured control and explicit local-result binding obligations.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

/// These controls perform physical movement only through execute_effects
/// child frames. Their parent must not publish the same net departure again.
pub(super) fn movement_is_owned_by_child_frames(effect: &Value) -> bool {
    matches!(
        value_kind(effect),
        Some(
            "forEachObject"
                | "chooseOneEffect"
                | "optionalEffects"
                | "optionalAction"
                | "conditional"
                | "conditionalEffect"
                | "ifTargetWasChosen"
                | "chooseUnusedMode"
        )
    )
}

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    matches!(value_kind(value), Some("conditional" | "conditionalEffect"))
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("bind") => {
            effect["id"].is_string()
                && (player_reference_supported(&effect["value"])
                    || target_reference_supported(&effect["value"])
                    || object_expression_supported(&effect["value"])
                    || numeric_expression_supported(&effect["value"]))
        }
        Some("optionalAction") => {
            player_reference_supported(&effect["player"])
                && effect_supported(&effect["action"])
                && super::super::optional_action::optional_action_witness_supported(
                    &effect["action"],
                )
                && effect["onPerformed"]
                    .as_array()
                    .is_some_and(|effects| effects.iter().all(effect_supported))
                && effect.get("onDeclined").is_none_or(|effects| {
                    effects
                        .as_array()
                        .is_some_and(|effects| effects.iter().all(effect_supported))
                })
        }
        Some("optionalEffects") => {
            player_reference_supported(&effect["player"])
                && effect.get("revealSource").is_none_or(Value::is_boolean)
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        Some("chooseOneEffect") => {
            player_reference_supported(&effect["player"])
                && effect["options"].as_array().is_some_and(|options| {
                    options.len() >= 2
                        && options.iter().all(|option| {
                            option["label"].is_string()
                                && option.get("availableIf").is_none_or(condition_supported)
                                && option["effects"].as_array().is_some_and(|effects| {
                                    !effects.is_empty() && effects.iter().all(effect_supported)
                                })
                        })
                })
        }
        Some("forEachObject") => {
            object_expression_supported(&effect["objects"])
                && effect["bind"].is_string()
                && effect["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
        }
        Some("conditional" | "conditionalEffect") => {
            condition_supported(&effect["condition"])
                && ["then", "else"]
                    .iter()
                    .filter_map(|key| effect[*key].as_array())
                    .flatten()
                    .all(effect_supported)
        }
        Some("ifTargetWasChosen") => {
            target_reference_supported(&effect["target"])
                && effect["then"]
                    .as_array()
                    .is_some_and(|effects| effects.iter().all(effect_supported))
                && effect["else"]
                    .as_array()
                    .is_none_or(|effects| effects.iter().all(effect_supported))
        }
        Some("chooseUnusedMode") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && effect["modes"].as_array().is_some_and(|modes| {
                    modes.len() >= 2
                        && modes.iter().all(|mode| {
                            mode["id"].is_string()
                                && mode["label"].is_string()
                                && mode["effects"].as_array().is_some_and(|effects| {
                                    !effects.is_empty() && effects.iter().all(effect_supported)
                                })
                        })
                })
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_control_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_control_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_control_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("forEachObject") => {
                let objects = self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                let Some(binding_id) = effect["bind"].as_str() else {
                    return Ok(());
                };
                let nested = effect["effects"].as_array().cloned().unwrap_or_default();
                let prior_binding = bindings.get(binding_id).cloned();
                for instance_id in objects {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Target(TargetRef::Permanent { instance_id }),
                    );
                    self.execute_effects(
                        &Value::Array(nested.clone()),
                        stack_object,
                        bindings,
                        decisions,
                        provider,
                    )?;
                }
                if let Some(prior) = prior_binding {
                    bindings.insert(binding_id.to_string(), prior);
                } else {
                    bindings.remove(binding_id);
                }
            }
            Some("chooseOneEffect") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let options = effect["options"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|option| {
                        option.get("availableIf").is_none_or(|condition| {
                            self.runtime_condition_value(
                                condition,
                                stack_object,
                                bindings,
                                decisions,
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                let labels = options
                    .iter()
                    .filter_map(|option| option["label"].as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>();
                let Some(choice) = self.choose_resolution_option(
                    stack_object,
                    provider,
                    &player_id,
                    labels,
                    "Choose one effect.",
                )?
                else {
                    return Ok(());
                };
                let Some(chosen) = options
                    .iter()
                    .find(|option| option["label"].as_str() == Some(choice.as_str()))
                else {
                    return Ok(());
                };
                let nested = chosen["effects"].as_array().cloned().unwrap_or_default();
                self.execute_effects(
                    &Value::Array(nested.clone()),
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
            }
            Some("optionalEffects") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let effects = effect["effects"].as_array().cloned().unwrap_or_default();
                let is_library_shuffle = effects.len() == 1
                    && value_kind(&effects[0]) == Some("shuffleZone")
                    && value_kind(&effects[0]["zone"]) == Some("library");
                let perform = if is_library_shuffle {
                    self.choose_resolution_option(
                        stack_object,
                        provider,
                        &player_id,
                        vec![
                            "Keep the current order".to_string(),
                            "Shuffle the library".to_string(),
                        ],
                        "Shuffle the library?",
                    )?
                    .as_deref()
                        == Some("Shuffle the library")
                } else {
                    self.confirm_resolution_action(
                        stack_object,
                        provider,
                        &player_id,
                        "Perform the optional effects",
                    )?
                };
                if !perform {
                    return Ok(());
                }
                if effect["revealSource"].as_bool() == Some(true) {
                    let owner_id = stack_object.card.owner.clone();
                    let viewer_ids = self
                        .state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>();
                    for viewer_id in viewer_ids {
                        self.remember_known_hand_card(
                            &owner_id,
                            &viewer_id,
                            &stack_object.card.instance_id,
                        );
                    }
                    self.record_event(
                        "cardRevealed",
                        Some(owner_id),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "zone": "hand", "reason": "openingHandAbility" }),
                    );
                }
                self.execute_effects(
                    &Value::Array(effects.clone()),
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
            }
            Some("optionalAction") => {
                self.execute_optional_action(effect, stack_object, bindings, decisions, provider)?;
            }
            Some("bind") => {
                let Some(id) = effect["id"].as_str() else {
                    return Ok(());
                };
                if let Some(target) = self.resolve_target(&effect["value"], stack_object, bindings)
                {
                    bindings.insert(id.to_string(), RuntimeBinding::Target(target));
                } else if let Some(player_id) =
                    self.resolve_player(&effect["value"], stack_object, bindings)
                {
                    bindings.insert(id.to_string(), RuntimeBinding::Player(player_id));
                } else if let Some(objects) = self.resolve_runtime_object_ids(
                    &effect["value"],
                    stack_object,
                    bindings,
                    decisions,
                ) {
                    bindings.insert(id.to_string(), RuntimeBinding::Objects(objects));
                } else if let Some(value) = self.runtime_numeric_expression(
                    &effect["value"],
                    stack_object,
                    bindings,
                    decisions,
                ) {
                    bindings.insert(id.to_string(), RuntimeBinding::Integer(value));
                }
            }
            Some("conditional" | "conditionalEffect") => {
                super::super::resolution_costs::validate_required_results(
                    &effect["condition"],
                    decisions,
                )?;
                if self.condition_requires_initially_illegal_target(
                    &effect["condition"],
                    stack_object,
                    bindings,
                    decisions,
                    0,
                )? {
                    return Ok(());
                }
                let branch = if self.checked_runtime_numeric_condition(
                    &effect["condition"],
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    &effect["then"]
                } else {
                    &effect["else"]
                };
                self.execute_effects(branch, stack_object, bindings, decisions, provider)?;
            }
            Some("ifTargetWasChosen") => {
                let branch = if self
                    .resolve_target(&effect["target"], stack_object, bindings)
                    .is_some()
                {
                    &effect["then"]
                } else {
                    &effect["else"]
                };
                self.execute_effects(branch, stack_object, bindings, decisions, provider)?;
            }
            Some("chooseUnusedMode") => {
                let Some(ability_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let available = effect["modes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|mode| {
                        let mode_id = mode["id"].as_str();
                        !self.state.rule_modifiers.iter().any(|modifier| {
                            value_kind(modifier) == Some("usedPersistentMode")
                                && modifier["sourceCardInstanceId"].as_str()
                                    == Some(stack_object.card.instance_id.as_str())
                                && modifier["abilityId"].as_str() == Some(ability_id)
                                && modifier["modeId"].as_str() == mode_id
                        })
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let labels = available
                    .iter()
                    .filter_map(|mode| mode["label"].as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>();
                let Some(selected_label) = self.choose_resolution_option(
                    stack_object,
                    provider,
                    &player_id,
                    labels,
                    "Choose a mode that has not been chosen.",
                )?
                else {
                    return Ok(());
                };
                let Some(selected) = available
                    .into_iter()
                    .find(|mode| mode["label"].as_str() == Some(selected_label.as_str()))
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "usedPersistentMode",
                    "sourceCardInstanceId": stack_object.card.instance_id,
                    "abilityId": ability_id,
                    "modeId": selected["id"],
                }));
                self.execute_effects(
                    &selected["effects"],
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
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
