//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("putCounters") => {
            let permanent = &effect["permanent"];
            let supported = if value_kind(permanent) == Some("eachPermanent") {
                counter_selection::counter_collection_selector_supported(permanent)
            } else {
                matches!(value_kind(permanent), Some("self" | "abilitySource"))
                    || entry_observation::observed_object_reference_supported(permanent)
                    || target_reference_supported(permanent)
                    || object_expression_supported(permanent)
            };
            supported
                && effect["counter"].is_string()
                && numeric_expression_supported(&effect["count"])
                && effect
                    .get("placedBy")
                    .is_none_or(singular_player_reference_supported)
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("putCountersOnCreaturesOfTargetPlayer") => {
            target_reference_supported(&effect["player"])
                && effect["counter"].is_string()
                && numeric_expression_supported(&effect["count"])
        }
        Some("removeCounters") => {
            matches!(
                value_kind(&effect["permanent"]),
                Some("self" | "abilitySource")
            ) && effect["counter"].is_string()
                && integer_value(&effect["count"]).is_some_and(|count| count > 0)
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("removeUpToCounters") => {
            target_reference_supported(&effect["permanent"])
                && player_reference_supported(&effect["player"])
                && effect
                    .get("minimum")
                    .is_none_or(|minimum| integer_value(minimum).is_some_and(|value| value >= 0))
                && integer_value(&effect["maximum"]).is_some_and(|maximum| maximum > 0)
                && integer_value(&effect["minimum"]).unwrap_or(0)
                    <= integer_value(&effect["maximum"]).unwrap_or(0)
        }
        Some("removeAnyNumberOfCounters") => {
            target_reference_supported(&effect["permanent"])
                && player_reference_supported(&effect["player"])
        }
        Some("blightPlayer") => {
            player_reference_supported(&effect["player"])
                && integer_value(&effect["count"]).is_some_and(|count| count > 0)
        }
        Some("blightEachOpponent") => {
            integer_value(&effect["count"]).is_some_and(|count| count > 0)
        }
        Some("removeAllCounters") => target_reference_supported(&effect["permanent"]),
        Some("putSameCountersAs") => {
            (target_reference_supported(&effect["permanent"])
                || matches!(
                    value_kind(&effect["permanent"]),
                    Some("self" | "abilitySource")
                ))
                && matches!(
                    value_kind(&effect["source"]),
                    Some("self" | "abilitySource" | "triggeringPermanent")
                )
        }
        Some("doubleCounters") => {
            (matches!(
                value_kind(&effect["permanent"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["permanent"]))
                && effect["counter"].is_string()
        }
        Some("doubleAllCountersOnPermanents") => {
            player_reference_supported(&effect["player"]) && card_filter_supported(&effect["where"])
        }
        Some("addPlayerCounters") => {
            player_reference_supported(&effect["player"])
                && effect["counter"].is_string()
                && numeric_expression_supported(&effect["count"])
        }
        Some("removePlayerCounters") => {
            player_reference_supported(&effect["player"])
                && effect["counter"].is_string()
                && numeric_expression_supported(&effect["count"])
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_counters_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_counters_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_counters_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("putCounters") => {
                let mut excluded_instance_ids = effect["permanent"]
                    .get("exclude")
                    .and_then(|exclude| {
                        self.resolve_runtime_object_ids(exclude, stack_object, bindings, decisions)
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                if effect["permanent"]["excludeSource"].as_bool() == Some(true) {
                    excluded_instance_ids.insert(stack_object.card.instance_id.clone());
                }
                let instance_ids = match self.prepare_permanent_action_subject(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    permanent_subject::PermanentSubjectResolution::Unavailable => {
                        if let Some(id) = effect.get("bind").and_then(Value::as_str) {
                            bindings.insert(id.to_owned(), RuntimeBinding::Objects(Vec::new()));
                        }
                        return Ok(());
                    }
                    permanent_subject::PermanentSubjectResolution::Resolved(ids) => ids,
                    permanent_subject::PermanentSubjectResolution::Historical => {
                        match value_kind(&effect["permanent"]) {
                            Some("self" | "abilitySource") => {
                                vec![stack_object.card.instance_id.clone()]
                            }
                            Some("chosenTargets") => self
                                .resolve_runtime_object_ids(
                                    &effect["permanent"],
                                    stack_object,
                                    bindings,
                                    decisions,
                                )
                                .unwrap_or_default(),
                            Some(
                                "boundObjects" | "decisionResult" | "setDifference"
                                | "filterObjects",
                            ) => self
                                .resolve_runtime_object_ids(
                                    &effect["permanent"],
                                    stack_object,
                                    bindings,
                                    decisions,
                                )
                                .unwrap_or_default(),
                            Some("eachPermanent") => self.counter_collection_ids(
                                &effect["permanent"],
                                stack_object,
                                bindings,
                                &excluded_instance_ids,
                            )?,
                            _ => self
                                .resolve_target(&effect["permanent"], stack_object, bindings)
                                .and_then(|target| match target {
                                    TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                                    _ => None,
                                })
                                .unwrap_or_default(),
                        }
                    }
                };
                let counter = effect["counter"]
                    .as_str()
                    .ok_or_else(|| EngineError::new("counter placement has no counter name"))?
                    .to_string();
                let placed_by = match effect.get("placedBy") {
                    None => stack_object.controller.clone(),
                    Some(player) => self
                        .resolve_player(player, stack_object, bindings)
                        .ok_or_else(|| {
                            EngineError::new("explicit counter placement actor is unresolved")
                        })?,
                };
                self.player_index(&placed_by)?;
                let count = self
                    .checked_runtime_numeric_expression(
                        &effect["count"],
                        stack_object,
                        bindings,
                        decisions,
                        "putCounters.count",
                    )?
                    .max(0);
                let mut changed_instance_ids = Vec::new();
                for (instance_id, actual) in self.place_counters_batch(
                    instance_ids,
                    &counter,
                    count,
                    QuantityOrigin::Effect,
                    Some(&placed_by),
                    provider,
                )? {
                    if actual > 0 {
                        changed_instance_ids.push(instance_id);
                    }
                }
                if let Some(binding_id) = effect.get("bind").and_then(Value::as_str) {
                    bindings.insert(
                        binding_id.to_string(),
                        RuntimeBinding::Objects(changed_instance_ids),
                    );
                }
            }
            Some("putCountersOnCreaturesOfTargetPlayer") => {
                let target_player = self
                    .resolve_target(&effect["player"], stack_object, bindings)
                    .and_then(|target| match target {
                        TargetRef::Player { player_id } => Some(player_id),
                        _ => None,
                    });
                let player_id = if let Some(player_id) = target_player {
                    player_id
                } else if let Some(player_id) = self.choose_resolution_player(
                    stack_object,
                    provider,
                    &stack_object.controller,
                    false,
                    false,
                    "Choose a player whose creatures receive counters.",
                )? {
                    player_id
                } else {
                    return Ok(());
                };
                let counter = effect["counter"].as_str().unwrap_or("+1/+1");
                let count = self
                    .runtime_numeric_expression(
                        effect.get("count").unwrap_or(&Value::Null),
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(1);
                let instance_ids = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| {
                        permanent.controller == player_id && self.permanent_is_creature(permanent)
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                for instance_id in instance_ids {
                    self.place_counters(
                        &instance_id,
                        counter,
                        count,
                        QuantityOrigin::Effect,
                        None,
                        provider,
                    )?;
                }
            }
            Some("putSameCountersAs") => {
                let declared = match value_kind(&effect["permanent"]) {
                    Some("self" | "abilitySource") => Some(stack_object.card.instance_id.clone()),
                    _ => self
                        .resolve_target(&effect["permanent"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        }),
                };
                let instance_id = if let Some(instance_id) = declared {
                    instance_id
                } else if value_kind(&effect["candidates"]) == Some("permanents") {
                    let candidates = self
                        .state
                        .players
                        .iter()
                        .flat_map(|player| player.battlefield.iter())
                        .filter(|permanent| {
                            permanent.controller == stack_object.controller
                                && matches_permanent_filter(
                                    permanent,
                                    &effect["candidates"]["where"],
                                )
                        })
                        .map(|permanent| permanent.instance_id.clone())
                        .collect::<Vec<_>>();
                    let selected = self.choose_resolution_objects(
                        stack_object,
                        provider,
                        &stack_object.controller,
                        "counterRecipient",
                        candidates,
                        1,
                        1,
                        "Choose a creature to receive the counters.",
                    )?;
                    let Some(instance_id) = selected.into_iter().next() else {
                        return Ok(());
                    };
                    instance_id
                } else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let source_counters =
                    if value_kind(&effect["source"]) == Some("triggeringPermanent") {
                        stack_object.decisions["triggeringPermanentCounters"]
                            .as_object()
                            .map(|counters| {
                                counters
                                    .iter()
                                    .filter_map(|(counter, count)| {
                                        count.as_i64().and_then(|count| {
                                            i32::try_from(count).ok().map(|count| (counter, count))
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    } else {
                        stack_object
                            .card
                            .counters
                            .iter()
                            .map(|(counter, count)| (counter, *count))
                            .collect::<Vec<_>>()
                    };
                for (counter, count) in source_counters {
                    *self.state.players[player_index].battlefield[card_index]
                        .counters
                        .entry(counter.clone())
                        .or_default() += count;
                }
            }
            Some("doubleCounters") => {
                let instance_id = match value_kind(&effect["permanent"]) {
                    Some("self" | "abilitySource") => stack_object.card.instance_id.clone(),
                    _ => self
                        .resolve_target(&effect["permanent"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        })
                        .unwrap_or_default(),
                };
                if let Some((player_index, card_index)) = self.permanent_position(&instance_id) {
                    let counter = effect["counter"].as_str().unwrap_or("+1/+1");
                    let current = self.state.players[player_index].battlefield[card_index]
                        .counters
                        .get(counter)
                        .copied()
                        .unwrap_or(0);
                    if current > 0 {
                        self.place_counters(
                            &instance_id,
                            counter,
                            current,
                            QuantityOrigin::Effect,
                            None,
                            provider,
                        )?;
                    }
                }
            }
            Some("doubleAllCountersOnPermanents") => {
                let ids = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| {
                        self.event_player_reference_matches(
                            &effect["player"],
                            &stack_object.card,
                            &permanent.controller,
                        ) && matches_card_filter(&permanent.definition, &effect["where"])
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                for id in ids {
                    let counters = self
                        .permanent_position(&id)
                        .map(|(player_index, card_index)| {
                            self.state.players[player_index].battlefield[card_index]
                                .counters
                                .clone()
                        })
                        .unwrap_or_default();
                    for (counter, count) in counters {
                        if count > 0 {
                            self.place_counters(
                                &id,
                                &counter,
                                count,
                                QuantityOrigin::Effect,
                                None,
                                provider,
                            )?;
                        }
                    }
                }
            }
            Some("removeCounters") => {
                let instance_id = match value_kind(&effect["permanent"]) {
                    Some("self" | "abilitySource") => stack_object.card.instance_id.clone(),
                    _ => return Ok(()),
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let counter = effect["counter"].as_str().unwrap_or_default();
                let count = integer_value(&effect["count"]).unwrap_or(0).max(0);
                let counters = self.state.players[player_index].battlefield[card_index]
                    .counters
                    .entry(counter.to_string())
                    .or_default();
                let removed = *counters >= count;
                if removed {
                    *counters -= count;
                }
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(
                        binding.to_string(),
                        RuntimeBinding::Objects(if removed {
                            vec![instance_id]
                        } else {
                            Vec::new()
                        }),
                    );
                }
            }
            Some("removeUpToCounters") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(chooser_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let minimum = integer_value(&effect["minimum"]).unwrap_or(0).max(0);
                let maximum = integer_value(&effect["maximum"]).unwrap_or(0).max(0);
                for index in 0..maximum {
                    let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    else {
                        break;
                    };
                    let mut options = self.state.players[player_index].battlefield[card_index]
                        .counters
                        .iter()
                        .filter(|(_, count)| **count > 0)
                        .map(|(counter, _)| counter.clone())
                        .collect::<Vec<_>>();
                    options.sort();
                    if index >= minimum {
                        options.push("done".to_string());
                    }
                    let Some(counter) = self.choose_resolution_option(
                        stack_object,
                        provider,
                        &chooser_id,
                        options,
                        "Choose a counter to remove, or finish.",
                    )?
                    else {
                        break;
                    };
                    if counter == "done" {
                        break;
                    }
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                        && let Some(count) = self.state.players[player_index].battlefield
                            [card_index]
                            .counters
                            .get_mut(&counter)
                    {
                        *count = count.saturating_sub(1);
                    }
                }
            }
            Some("removeAnyNumberOfCounters") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(chooser_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                loop {
                    let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    else {
                        break;
                    };
                    let mut options = self.state.players[player_index].battlefield[card_index]
                        .counters
                        .iter()
                        .filter(|(_, count)| **count > 0)
                        .map(|(counter, _)| counter.clone())
                        .collect::<Vec<_>>();
                    options.sort();
                    options.push("done".to_string());
                    let Some(counter) = self.choose_resolution_option(
                        stack_object,
                        provider,
                        &chooser_id,
                        options,
                        "Choose a counter to remove, or finish.",
                    )?
                    else {
                        break;
                    };
                    if counter == "done" {
                        break;
                    }
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                        && let Some(count) = self.state.players[player_index].battlefield
                            [card_index]
                            .counters
                            .get_mut(&counter)
                    {
                        *count = count.saturating_sub(1);
                    }
                }
            }
            Some("blightPlayer") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let count = integer_value(&effect["count"]).unwrap_or(0).max(0);
                let Some(player_index) = self.player_index(&player_id).ok() else {
                    return Ok(());
                };
                let options = self.state.players[player_index]
                    .battlefield
                    .iter()
                    .filter(|permanent| {
                        permanent.controller == player_id && self.permanent_is_creature(permanent)
                    })
                    .map(|permanent| LegalAction {
                        id: format!("blight:{}:{}", stack_object.id, permanent.instance_id),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: format!(
                            "Put {count} -1/-1 counter(s) on {}",
                            permanent.definition.name
                        ),
                        card_instance_id: Some(stack_object.card.instance_id.clone()),
                        payment_sources: Vec::new(),
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::from([(
                            "blightedPermanent".to_string(),
                            TargetRef::Permanent {
                                instance_id: permanent.instance_id.clone(),
                            },
                        )]),
                        target_order: vec!["blightedPermanent".to_string()],
                        attacker_id: None,
                        blocker_id: None,
                    })
                    .collect::<Vec<_>>();
                if options.is_empty() {
                    return Ok(());
                }
                let choice = self.choose_action(
                    provider,
                    EngineDecisionRequest {
                        id: format!("blight:{}", stack_object.id),
                        kind: DecisionKind::ResolutionChoice,
                        player_id,
                        source_card: Some(stack_object.card.clone()),
                        source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                        choice: None,
                        options,
                    },
                )?;
                if let Some(TargetRef::Permanent { instance_id }) =
                    choice.targets.get("blightedPermanent")
                {
                    let instance_id = instance_id.clone();
                    self.place_counters(
                        &instance_id,
                        "-1/-1",
                        count,
                        QuantityOrigin::Effect,
                        None,
                        provider,
                    )?;
                }
            }
            Some("blightEachOpponent") => {
                let count = integer_value(&effect["count"]).unwrap_or(0).max(0);
                let opponent_ids = self
                    .state
                    .players
                    .iter()
                    .filter(|player| !player.has_lost && player.id != stack_object.controller)
                    .map(|player| player.id.clone())
                    .collect::<Vec<_>>();
                for player_id in opponent_ids {
                    let Some(player_index) = self.player_index(&player_id).ok() else {
                        continue;
                    };
                    let options = self.state.players[player_index]
                        .battlefield
                        .iter()
                        .filter(|permanent| self.permanent_is_creature(permanent))
                        .map(|permanent| LegalAction {
                            id: format!("blight:{}:{}", stack_object.id, permanent.instance_id),
                            kind: ActionKind::ChooseResolution,
                            player_id: player_id.clone(),
                            label: format!(
                                "Put {count} -1/-1 counter(s) on {}",
                                permanent.definition.name
                            ),
                            card_instance_id: Some(stack_object.card.instance_id.clone()),
                            payment_sources: Vec::new(),
                            decisions: BTreeMap::new(),
                            targets: BTreeMap::from([(
                                "blightedPermanent".to_string(),
                                TargetRef::Permanent {
                                    instance_id: permanent.instance_id.clone(),
                                },
                            )]),
                            target_order: vec!["blightedPermanent".to_string()],
                            attacker_id: None,
                            blocker_id: None,
                        })
                        .collect::<Vec<_>>();
                    if options.is_empty() {
                        continue;
                    }
                    let choice = self.choose_action(
                        provider,
                        EngineDecisionRequest {
                            id: format!("blight:{}:{player_id}", stack_object.id),
                            kind: DecisionKind::ResolutionChoice,
                            player_id,
                            source_card: Some(stack_object.card.clone()),
                            source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                            choice: None,
                            options,
                        },
                    )?;
                    if let Some(TargetRef::Permanent { instance_id }) =
                        choice.targets.get("blightedPermanent")
                    {
                        self.place_counters(
                            instance_id,
                            "-1/-1",
                            count,
                            QuantityOrigin::Effect,
                            None,
                            provider,
                        )?;
                    }
                }
            }
            Some("removeAllCounters") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let removed = std::mem::take(
                    &mut self.state.players[player_index].battlefield[card_index].counters,
                );
                for (counter, count) in removed.into_iter().filter(|(_, count)| *count > 0) {
                    self.record_event(
                        "countersRemoved",
                        Some(stack_object.controller.clone()),
                        Some(instance_id.clone()),
                        json!({ "counter": counter, "count": count }),
                    );
                }
            }
            Some("addPlayerCounters") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let counter = effect["counter"].as_str().unwrap_or_default().to_string();
                let count = self
                    .runtime_numeric_expression(&effect["count"], stack_object, bindings, decisions)
                    .unwrap_or(0)
                    .max(0);
                if !counter.is_empty() && count > 0 {
                    *self.state.players[player_index]
                        .counters
                        .entry(counter.clone())
                        .or_default() += count;
                    self.record_event(
                        "playerCountersAdded",
                        Some(player_id),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "count": count, "counter": counter }),
                    );
                }
            }
            Some("removePlayerCounters") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let counter = effect["counter"].as_str().unwrap_or_default().to_string();
                let requested = self
                    .runtime_numeric_expression(&effect["count"], stack_object, bindings, decisions)
                    .unwrap_or(0)
                    .max(0);
                let available = self.state.players[player_index]
                    .counters
                    .get(&counter)
                    .copied()
                    .unwrap_or(0);
                let removed = requested.min(available);
                if !counter.is_empty() && removed > 0 {
                    self.state.players[player_index]
                        .counters
                        .insert(counter.clone(), available - removed);
                    self.record_event(
                        "playerCountersRemoved",
                        Some(player_id),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "count": removed, "counter": counter }),
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
