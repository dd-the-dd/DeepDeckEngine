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
        // CR 608.2d: one placement the controller chooses on resolution;
        // each option is an ordinary placement on the same object.
        Some("putChosenCounters") => {
            effect["options"].as_array().is_some_and(|options| {
                let mut kinds = std::collections::BTreeSet::new();
                options.len() >= 2
                    && options.iter().all(|option| {
                        option["counter"]
                            .as_str()
                            .is_some_and(|counter| kinds.insert(counter.to_string()))
                            && numeric_expression_supported(&option["count"])
                    })
            }) && {
                let mut placement = effect.clone();
                placement["kind"] = json!("putCounters");
                placement["counter"] = effect["options"][0]["counter"].clone();
                placement["count"] = effect["options"][0]["count"].clone();
                if let Some(object) = placement.as_object_mut() {
                    object.remove("options");
                }
                supports_known_effect(&placement)
            }
        }
        // CR 601.2d: the division announced with the targets.
        Some("putDividedCounters") => {
            effect["targetsDecisionId"].is_string()
                && effect["divisionDecisionId"].is_string()
                && effect["counter"].is_string()
        }
        Some("putCountersOnCreaturesOfTargetPlayer") => {
            target_reference_supported(&effect["player"])
                && effect["counter"].is_string()
                && numeric_expression_supported(&effect["count"])
        }
        Some("removeCounters") => quantity_cost_payers::counter_removal_supported(effect),
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
        // CR 701.34a: the proliferating player chooses the recipients.
        Some("proliferate") => singular_player_reference_supported(&effect["player"]),
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
            // CR 608.2d: the controller chooses one placement among the
            // options, then that ordinary placement happens.
            Some("putChosenCounters") => {
                let options = effect["options"].as_array().cloned().unwrap_or_default();
                let labels = options
                    .iter()
                    .filter_map(|option| option["counter"].as_str().map(str::to_string))
                    .collect::<Vec<_>>();
                let Some(chosen) = self.choose_resolution_option(
                    stack_object,
                    provider,
                    &stack_object.controller,
                    labels,
                    "Choose which kind of counter to put.",
                )?
                else {
                    return Ok(());
                };
                let option = options
                    .iter()
                    .find(|option| option["counter"].as_str() == Some(chosen.as_str()))
                    .ok_or_else(|| EngineError::new("the chosen counter kind is not an option"))?;
                let mut placement = effect.clone();
                placement["kind"] = json!("putCounters");
                placement["counter"] = option["counter"].clone();
                placement["count"] = option["count"].clone();
                if let Some(object) = placement.as_object_mut() {
                    object.remove("options");
                }
                self.execute_counters_domain_known(
                    &placement,
                    stack_object,
                    bindings,
                    decisions,
                    provider,
                )?;
            }
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
            Some("putDividedCounters") => {
                self.put_divided_counters(effect, stack_object, decisions, provider)?;
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
                        ) && matches_card_filter(
                            object_predicate::DefinitionView::for_permanent(permanent),
                            &effect["where"],
                        )
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
                let Some(instance_id) =
                    self.counter_removal_instance(effect, stack_object, bindings)
                else {
                    return Ok(());
                };
                // The ability's own object: a permanent, or a card whose
                // ability functions in exile (CR 113.6k, e.g. Suspend).
                let location = match self.permanent_position(&instance_id) {
                    Some(position) => Some(("battlefield", position)),
                    None if instance_id == stack_object.card.instance_id => self
                        .exiled_object_position(&stack_object.card)
                        .map(|position| ("exile", position)),
                    None => None,
                };
                let Some((zone, (player_index, card_index))) = location else {
                    return Ok(());
                };
                let counter = effect["counter"].as_str().unwrap_or_default();
                let count = self.counter_removal_count(effect, stack_object, bindings, decisions);
                let object = match zone {
                    "battlefield" => &mut self.state.players[player_index].battlefield[card_index],
                    _ => &mut self.state.players[player_index].exile[card_index],
                };
                let counters = object.counters.entry(counter.to_string()).or_default();
                let removed = *counters >= count;
                if removed {
                    *counters -= count;
                }
                let remaining = *counters;
                if removed && count > 0 {
                    let object = object.clone();
                    self.record_event(
                        "counterRemoved",
                        Some(stack_object.controller.clone()),
                        Some(instance_id.clone()),
                        json!({ "counter": counter, "count": count, "remaining": remaining }),
                    );
                    self.enqueue_counter_removed_triggers(&object, zone, counter, remaining);
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
            Some("proliferate") => {
                self.proliferate(effect, stack_object, bindings, decisions, provider)?;
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

/// Every subset of `objects` (smallest first), capped for the decision menu.
fn proliferate_subsets(objects: &[String], maximum: usize) -> Vec<Vec<String>> {
    let mut subsets = Vec::new();
    for size in 0..=objects.len() {
        subsets.extend(GameEngine::resolution_selections(
            objects,
            size,
            maximum.saturating_sub(subsets.len()),
        ));
        if subsets.len() >= maximum {
            break;
        }
    }
    subsets
}

fn selected_ids(decisions: &BTreeMap<String, Value>, decision_id: &str) -> Vec<String> {
    decisions
        .get(decision_id)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

fn positive_counter_kinds(counters: &BTreeMap<String, i32>) -> Vec<String> {
    counters
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(counter, _)| counter.clone())
        .collect()
}

impl GameEngine {
    /// CR 701.34a: choose any number of permanents and/or players that have a
    /// counter, then give each one additional counter of each kind it already
    /// has. Permanents and players are two typed selections of one choice;
    /// permanent counters use the ordinary placement primitive.
    fn proliferate<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let player_id = self
            .resolve_player(&effect["player"], stack_object, bindings)
            .ok_or_else(|| EngineError::new("proliferate has no acting player"))?;
        self.player_index(&player_id)?;
        let permanent_candidates = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|permanent| !positive_counter_kinds(&permanent.counters).is_empty())
            .map(|permanent| permanent.instance_id.clone())
            .collect::<Vec<_>>();
        let player_candidates = self
            .state
            .players
            .iter()
            .filter(|player| {
                !player.has_lost && !positive_counter_kinds(&player.counters).is_empty()
            })
            .map(|player| player.id.clone())
            .collect::<Vec<_>>();
        let sequence = self.state.events.len();
        let mut chosen_permanents = Vec::new();
        if !permanent_candidates.is_empty() {
            let decision_id = format!("proliferatePermanents:{}:{sequence}", stack_object.id);
            let values = proliferate_subsets(&permanent_candidates, 128);
            self.make_resolution_decision(
                &decision_id,
                &player_id,
                stack_object,
                DecisionChoice::CardSelection {
                    decision_id: decision_id.clone(),
                    candidate_card_instance_ids: permanent_candidates.clone(),
                    minimum: 0,
                    maximum: permanent_candidates.len(),
                    prompt: "Choose any number of permanents with counters to proliferate."
                        .to_string(),
                },
                values,
                decisions,
                provider,
            )?;
            chosen_permanents = selected_ids(decisions, &decision_id);
        }
        let mut chosen_players = Vec::new();
        if !player_candidates.is_empty() {
            let decision_id = format!("proliferatePlayers:{}:{sequence}", stack_object.id);
            let values = proliferate_subsets(&player_candidates, 128);
            self.make_resolution_decision(
                &decision_id,
                &player_id,
                stack_object,
                DecisionChoice::OptionSelection {
                    decision_id: decision_id.clone(),
                    options: player_candidates.clone(),
                    prompt: "Choose any number of players with counters to proliferate."
                        .to_string(),
                },
                values,
                decisions,
                provider,
            )?;
            chosen_players = selected_ids(decisions, &decision_id);
        }
        for instance_id in chosen_permanents {
            let kinds = self
                .permanent_position(&instance_id)
                .map(|(player_index, card_index)| {
                    positive_counter_kinds(
                        &self.state.players[player_index].battlefield[card_index].counters,
                    )
                })
                .unwrap_or_default();
            for counter in kinds {
                self.place_counters(
                    &instance_id,
                    &counter,
                    1,
                    QuantityOrigin::Effect,
                    Some(&player_id),
                    provider,
                )?;
            }
        }
        for chosen in chosen_players {
            let player_index = self.player_index(&chosen)?;
            for counter in positive_counter_kinds(&self.state.players[player_index].counters) {
                *self.state.players[player_index]
                    .counters
                    .entry(counter.clone())
                    .or_default() += 1;
                self.record_event(
                    "playerCountersAdded",
                    Some(chosen.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "count": 1, "counter": counter }),
                );
            }
        }
        self.record_event(
            "proliferated",
            Some(player_id.clone()),
            Some(stack_object.card.instance_id.clone()),
            json!({}),
        );
        self.publish_proliferated(&player_id);
        Ok(())
    }
}

impl GameEngine {
    /// `distribute N counters among TARGETS` (CR 601.2d): each declared
    /// target receives the share announced for it with the targets. A target
    /// no longer on the battlefield receives nothing (CR 608.2b); the shares
    /// of the others are unchanged.
    fn put_divided_counters<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        decisions: &BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let targets = effect["targetsDecisionId"]
            .as_str()
            .and_then(|decision_id| decisions.get(decision_id))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|target| serde_json::from_value::<TargetRef>(target.clone()).ok())
            .collect::<Vec<_>>();
        let division = effect["divisionDecisionId"]
            .as_str()
            .and_then(|decision_id| decisions.get(decision_id))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .filter_map(|amount| i32::try_from(amount).ok())
            .collect::<Vec<_>>();
        if targets.len() != division.len() || division.iter().any(|amount| *amount <= 0) {
            return Err(EngineError::new(
                "divided counters need one positive share per declared target",
            ));
        }
        let counter = effect["counter"]
            .as_str()
            .ok_or_else(|| EngineError::new("divided counters have no counter name"))?
            .to_string();
        let placed_by = stack_object.controller.clone();
        for (target, amount) in targets.into_iter().zip(division) {
            let TargetRef::Permanent { instance_id } = target else {
                continue;
            };
            if self.permanent_position(&instance_id).is_none() {
                continue;
            }
            self.place_counters_batch(
                vec![instance_id],
                &counter,
                amount,
                QuantityOrigin::Effect,
                Some(placed_by.as_str()),
                provider,
            )?;
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "counters_tests.rs"]
mod tests;
