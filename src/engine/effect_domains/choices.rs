//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("chooseCards") => {
            let bound_cardinality_supported = numeric_expression_supported(&effect["count"])
                || (numeric_expression_supported(&effect["minimum"])
                    && numeric_expression_supported(&effect["maximum"]));
            let bound_choice_supported = object_expression_supported(&effect["from"])
                && bound_cardinality_supported
                && effect.get("where").is_none_or(card_filter_supported);
            let candidate_choice_supported =
                card_choice_candidates_supported(&effect["candidates"])
                    && effect.get("sameLandType").is_none_or(Value::is_boolean)
                    && matches!(
                        (
                            numeric_expression_supported(&effect["minimum"]),
                            numeric_expression_supported(&effect["maximum"])
                        ),
                        (true, true)
                    );
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && effect
                    .get("onePerControlledColor")
                    .is_none_or(Value::is_boolean)
                && effect
                    .get("extremum")
                    .is_none_or(candidate_extremum_supported)
                && effect
                    .get("setConstraints")
                    .is_none_or(set_constraints_supported)
                && effect
                    .get("selection")
                    .is_none_or(|selection| value_kind(selection) == Some("random"))
                && (bound_choice_supported || candidate_choice_supported)
        }
        Some("choosePermanents") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && value_kind(&effect["candidates"]) == Some("permanents")
                && effect["candidates"]
                    .get("controller")
                    .is_none_or(player_reference_supported)
                && effect["candidates"]
                    .get("owner")
                    .is_none_or(player_reference_supported)
                && effect["candidates"]
                    .get("excludeSource")
                    .is_none_or(Value::is_boolean)
                && effect["candidates"]
                    .get("extremum")
                    .is_none_or(candidate_extremum_supported)
                && card_filter_supported(&effect["candidates"]["where"])
                && numeric_expression_supported(&effect["minimum"])
                && numeric_expression_supported(&effect["maximum"])
        }
        Some("chooseCardName") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && card_filter_supported(&effect["where"])
                && effect
                    .get("persistOn")
                    .is_none_or(|object| value_kind(object) == Some("self"))
        }
        Some("chooseCreatureType") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && effect.get("options").is_none_or(|options| {
                    nominal_choices::creature_type_options(Some(options)).is_ok()
                })
        }
        Some("choosePlayers") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && numeric_expression_supported(&effect["minimum"])
                && numeric_expression_supported(&effect["maximum"])
                && effect
                    .get("candidates")
                    .is_none_or(player_reference_supported)
                && effect
                    .get("selection")
                    .is_none_or(|selection| value_kind(selection) == Some("random"))
                && effect.get("bindAs").is_none_or(Value::is_string)
        }
        // CR 608.2d: one of the five colors, chosen while the effect resolves.
        Some("chooseColor") => {
            effect["id"].is_string() && player_reference_supported(&effect["player"])
        }
        // `the color or colors of your choice`: between `minimum` and
        // `maximum` distinct colors among the five (CR 105.1, 608.2d).
        Some("chooseColors") => {
            known_fields(effect, &["kind", "id", "player", "minimum", "maximum"])
                && effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && color_count_bounds(effect).is_some()
        }
        // `the basic land type of your choice` (CR 205.3i, 305.6).
        Some("chooseBasicLandType") => {
            known_fields(effect, &["kind", "id", "player"])
                && effect["id"].is_string()
                && player_reference_supported(&effect["player"])
        }
        Some("chooseNumber") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && numeric_expression_supported(&effect["minimum"])
                && numeric_expression_supported(&effect["maximum"])
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("chooseOrder") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && object_expression_supported(&effect["objects"])
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_choices_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_choices_domain_known(
            effect,
            next_effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_choices_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("choosePlayers") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                // The population the choice draws from (`an opponent`, `a
                // player`); every player in the game when unrestricted.
                let candidates = match effect.get("candidates") {
                    Some(population) => self.resolve_players(population, stack_object, bindings),
                    None => self
                        .state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>(),
                };
                let minimum = self
                    .runtime_numeric_expression(
                        &effect["minimum"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0)
                    .max(0) as usize;
                let maximum = self
                    .runtime_numeric_expression(
                        &effect["maximum"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0)
                    .max(0)
                    .min(candidates.len() as i32) as usize;
                let minimum = minimum.min(maximum);
                if value_kind(&effect["selection"]) == Some("random") {
                    // The game picks, not the chooser: no decision request.
                    let mut shuffled = candidates.clone();
                    shuffled.shuffle(&mut self.rng);
                    let selected = shuffled
                        .into_iter()
                        .take(maximum)
                        .map(Value::String)
                        .collect::<Vec<_>>();
                    decisions.insert(decision_id.to_string(), Value::Array(selected.clone()));
                    self.record_event(
                        "playersRandomlySelected",
                        Some(player_id),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "decisionId": decision_id, "playerIds": selected }),
                    );
                } else {
                    let mut values = Vec::new();
                    for count in minimum..=maximum {
                        values.extend(Self::resolution_selections(
                            &candidates,
                            count,
                            128 - values.len(),
                        ));
                    }
                    if values.iter().all(Vec::is_empty) {
                        // No player can be chosen: the instruction does as
                        // much as possible (CR 609.3) and binds no player.
                        decisions.insert(decision_id.to_string(), Value::Array(Vec::new()));
                    } else {
                        self.make_resolution_decision(
                            decision_id,
                            &player_id,
                            stack_object,
                            DecisionChoice::OptionSelection {
                                decision_id: decision_id.to_string(),
                                options: candidates,
                                prompt: "Choose players.".to_string(),
                            },
                            values,
                            decisions,
                            provider,
                        )?;
                    }
                }
                // A singular choice is the antecedent of a later `that
                // player`: it is bound as that player for this resolution.
                if let Some(binding_id) = effect["bindAs"].as_str() {
                    let chosen = decisions
                        .get(decision_id)
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToOwned::to_owned)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    match chosen.as_slice() {
                        [chosen] => {
                            bindings.insert(
                                binding_id.to_string(),
                                RuntimeBinding::Player(chosen.clone()),
                            );
                        }
                        _ => {
                            bindings.remove(binding_id);
                        }
                    }
                }
            }
            Some("chooseColor") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let colors = RESOLUTION_COLOR_NAMES
                    .iter()
                    .map(|color| (*color).to_string())
                    .collect::<Vec<_>>();
                let values = colors.iter().map(|color| vec![color.clone()]).collect();
                self.make_resolution_decision(
                    decision_id,
                    &player_id,
                    stack_object,
                    DecisionChoice::OptionSelection {
                        decision_id: decision_id.to_string(),
                        options: colors,
                        prompt: "Choose a color.".to_string(),
                    },
                    values,
                    decisions,
                    provider,
                )?;
                let chosen = resolution_choice_text(decisions.get(decision_id)).map(str::to_owned);
                self.record_event(
                    "colorChosen",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "decisionId": decision_id, "color": chosen }),
                );
            }
            Some("chooseColors") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let (minimum, maximum) = color_count_bounds(effect).ok_or_else(|| {
                    EngineError::new("chooseColors has invalid color-count bounds")
                })?;
                // Every set of distinct colors of an allowed size, in WUBRG
                // order; the recorded value is the color symbols `setColors`
                // reads.
                let selections = color_selections(minimum, maximum);
                let labels = selections
                    .iter()
                    .map(|symbols| {
                        symbols
                            .iter()
                            .map(|symbol| color_name_of_symbol(symbol))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .collect::<Vec<_>>();
                let prompt = if maximum == 1 {
                    "Choose a color."
                } else {
                    "Choose one or more colors."
                };
                self.make_resolution_decision(
                    decision_id,
                    &player_id,
                    stack_object,
                    DecisionChoice::OptionSelection {
                        decision_id: decision_id.to_string(),
                        options: labels,
                        prompt: prompt.to_string(),
                    },
                    selections,
                    decisions,
                    provider,
                )?;
                self.record_event(
                    "colorsChosen",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "decisionId": decision_id, "colors": decisions.get(decision_id) }),
                );
            }
            Some("chooseBasicLandType") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let types = super::super::subtypes::BASIC_LAND_TYPES
                    .iter()
                    .map(|land_type| (*land_type).to_string())
                    .collect::<Vec<_>>();
                let values = types
                    .iter()
                    .map(|land_type| vec![land_type.clone()])
                    .collect();
                self.make_resolution_decision(
                    decision_id,
                    &player_id,
                    stack_object,
                    DecisionChoice::OptionSelection {
                        decision_id: decision_id.to_string(),
                        options: types,
                        prompt: "Choose a basic land type.".to_string(),
                    },
                    values,
                    decisions,
                    provider,
                )?;
                let chosen = resolution_choice_text(decisions.get(decision_id)).map(str::to_owned);
                self.record_event(
                    "basicLandTypeChosen",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "decisionId": decision_id, "landType": chosen }),
                );
            }
            Some("chooseNumber") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let minimum = self
                    .runtime_numeric_expression(
                        &effect["minimum"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0);
                let maximum = self
                    .runtime_numeric_expression(
                        &effect["maximum"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(minimum)
                    .max(minimum);
                let selected = self.choose_number(
                    provider,
                    format!("resolution:{}:{decision_id}", stack_object.id),
                    player_id,
                    decision_id.to_string(),
                    minimum,
                    maximum,
                    "Choose an amount.".to_string(),
                    Some(stack_object.card.instance_id.clone()),
                )?;
                decisions.insert(decision_id.to_string(), Value::from(selected));
                // `that many` / `that much` reads the chosen number (CR 608.2d).
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(binding.to_string(), RuntimeBinding::Integer(selected));
                }
                self.record_event(
                    "resolutionChoiceMade",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    json!({
                        "decisionId": decision_id,
                        "selected": selected,
                        "stackId": stack_object.id,
                    }),
                );
            }
            Some("chooseCardName") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let chosen_name = self.make_card_name_decision(
                    decision_id,
                    &player_id,
                    stack_object,
                    &effect["where"],
                    decisions,
                    provider,
                )?;
                if value_kind(&effect["persistOn"]) == Some("self")
                    && let Some((player_index, card_index)) =
                        self.permanent_position(&stack_object.card.instance_id)
                {
                    let rules = &mut self.state.players[player_index].battlefield[card_index]
                        .definition
                        .printed_rules_mut();
                    rules.retain(|rule| {
                        value_kind(rule) != Some("storedDecisionValue")
                            || rule["decisionId"].as_str() != Some(decision_id)
                    });
                    rules.push(json!({
                        "kind": "storedDecisionValue",
                        "decisionId": decision_id,
                        "value": chosen_name.clone(),
                    }));
                }
                self.record_event(
                    "cardNameChosen",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({
                        "decisionId": decision_id,
                        "name": chosen_name,
                    }),
                );
            }
            Some("chooseCreatureType") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(chosen_type) = self.choose_creature_type_for_resolution(
                    stack_object,
                    provider,
                    &player_id,
                    "Choose a creature type.",
                    effect.get("options"),
                )?
                else {
                    return Ok(());
                };
                decisions.insert(
                    decision_id.to_string(),
                    Value::Array(vec![Value::String(chosen_type.clone())]),
                );
                self.record_event(
                    "creatureTypeChosen",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "decisionId": decision_id, "creatureType": chosen_type }),
                );
            }
            Some("choosePermanents") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(choosing_player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let candidate_owner_id = effect["candidates"]
                    .get("owner")
                    .and_then(|owner| self.resolve_player(owner, stack_object, bindings));
                let candidate_controller_id = effect["candidates"]
                    .get("controller")
                    .and_then(|controller| self.resolve_player(controller, stack_object, bindings));
                let objects = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|card| {
                        !card.flags.get("phasedOut").copied().unwrap_or(false)
                            && candidate_controller_id
                                .as_ref()
                                .is_none_or(|controller_id| card.controller == *controller_id)
                            && candidate_owner_id
                                .as_ref()
                                .is_none_or(|owner_id| card.owner == *owner_id)
                            && !(effect["candidates"]["excludeSource"].as_bool() == Some(true)
                                && card.instance_id == stack_object.card.instance_id)
                            && matches_permanent_filter(card, &effect["candidates"]["where"])
                    })
                    .map(|card| card.instance_id.clone())
                    .collect::<Vec<_>>();
                let objects = match effect["candidates"].get("extremum") {
                    Some(extremum) => self.retain_candidate_extremum(objects, extremum),
                    None => objects,
                };
                let minimum = self
                    .checked_runtime_numeric_expression(
                        &effect["minimum"],
                        stack_object,
                        bindings,
                        decisions,
                        "choosePermanents.minimum",
                    )?
                    .max(0) as usize;
                // Choosing N when fewer qualify chooses all of them; the
                // instruction is performed as far as possible (CR 609.3).
                let minimum = minimum.min(objects.len());
                let maximum = self
                    .checked_runtime_numeric_expression(
                        &effect["maximum"],
                        stack_object,
                        bindings,
                        decisions,
                        "choosePermanents.maximum",
                    )?
                    .max(0)
                    .min(objects.len() as i32) as usize;
                let mut values = Vec::new();
                for count in minimum..=maximum {
                    values.extend(Self::resolution_selections(
                        &objects,
                        count,
                        128 - values.len(),
                    ));
                    if values.len() >= 128 {
                        break;
                    }
                }
                self.make_resolution_decision(
                    decision_id,
                    &choosing_player_id,
                    stack_object,
                    DecisionChoice::CardSelection {
                        decision_id: decision_id.to_string(),
                        candidate_card_instance_ids: objects,
                        minimum,
                        maximum,
                        prompt: "Choose permanents for this effect.".to_string(),
                    },
                    values,
                    decisions,
                    provider,
                )?;
            }
            Some("chooseCards") => {
                if card_selection::ordinary_bound_selection(effect) {
                    return self.execute_bound_card_selection(
                        effect,
                        next_effect,
                        stack_object,
                        bindings,
                        decisions,
                        provider,
                    );
                }
                let random_selection = value_kind(&effect["selection"]) == Some("random");
                // Only random selection adopts the strict count boundary in
                // this patch. Existing nonrandom selection semantics stay put.
                let selection_bound = |expression: &Value, field: &'static str| {
                    let value = if random_selection {
                        self.checked_runtime_numeric_expression(
                            expression,
                            stack_object,
                            bindings,
                            decisions,
                            field,
                        )?
                    } else {
                        self.runtime_numeric_expression(
                            expression,
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .unwrap_or(0)
                    };
                    Ok::<usize, EngineError>(value.max(0) as usize)
                };
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let (objects, minimum, maximum) = if value_kind(&effect["candidates"])
                    == Some("cards")
                {
                    let candidate_player_id = effect["candidates"]["zone"]
                        .get("player")
                        .and_then(|zone_player| {
                            self.resolve_player(zone_player, stack_object, bindings)
                        })
                        .unwrap_or_else(|| player_id.clone());
                    let candidate_player_index = self.player_index(&candidate_player_id)?;
                    let candidate_zone = match value_kind(&effect["candidates"]["zone"]) {
                        Some("library") => &self.state.players[candidate_player_index].library,
                        Some("graveyard") => &self.state.players[candidate_player_index].graveyard,
                        Some("exile") => &self.state.players[candidate_player_index].exile,
                        Some("hand") => &self.state.players[candidate_player_index].hand,
                        Some("outsideGame") => {
                            &self.state.players[candidate_player_index].sideboard
                        }
                        _ => return Ok(()),
                    };
                    let prepared_filter = self.prepare_runtime_object_filter(
                        &effect["candidates"]["where"],
                        stack_object,
                        bindings,
                        decisions,
                    )?;
                    let where_filter = &prepared_filter;
                    let required_owner_id = (value_kind(where_filter) == Some("ownedBy"))
                        .then(|| {
                            self.resolve_player(&where_filter["player"], stack_object, bindings)
                        })
                        .flatten();
                    let objects = candidate_zone
                        .iter()
                        .filter(|card| match value_kind(where_filter) {
                            Some("ownedBy") => required_owner_id
                                .as_ref()
                                .is_some_and(|owner_id| card.owner == *owner_id),
                            _ => matches_card_filter_with_decisions(
                                &card.definition,
                                where_filter,
                                decisions,
                            ),
                        })
                        .map(|card| card.instance_id.clone())
                        .collect::<Vec<_>>();
                    (
                        objects,
                        selection_bound(&effect["minimum"], "chooseCards.minimum")?,
                        selection_bound(&effect["maximum"], "chooseCards.maximum")?,
                    )
                } else {
                    let mut objects = self
                        .resolve_runtime_object_ids(
                            &effect["from"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .unwrap_or_default();
                    if let Some(where_filter) = effect.get("where") {
                        objects.retain(|instance_id| {
                            self.runtime_collection_object_matches(
                                instance_id,
                                where_filter,
                                stack_object,
                                bindings,
                                decisions,
                            )
                        });
                    }
                    // `with the greatest power among ...`: the candidates
                    // ranked among themselves, ties kept (CR 107.1).
                    if let Some(extremum) = effect.get("extremum") {
                        objects = self.retain_candidate_extremum(objects, extremum);
                    }
                    if effect.get("minimum").is_some() || effect.get("maximum").is_some() {
                        (
                            objects,
                            selection_bound(&effect["minimum"], "chooseCards.minimum")?,
                            selection_bound(&effect["maximum"], "chooseCards.maximum")?,
                        )
                    } else {
                        let count = selection_bound(&effect["count"], "chooseCards.count")?;
                        (objects, count, count)
                    }
                };
                let effective_maximum = maximum.min(objects.len());
                let effective_minimum = minimum.min(effective_maximum);
                if random_selection {
                    let mut shuffled = objects.clone();
                    shuffled.shuffle(&mut self.rng);
                    let selected = shuffled
                        .into_iter()
                        .take(effective_maximum)
                        .map(Value::String)
                        .collect::<Vec<_>>();
                    decisions.insert(decision_id.to_string(), Value::Array(selected.clone()));
                    self.record_event(
                        "cardsRandomlySelected",
                        Some(player_id),
                        Some(stack_object.card.instance_id.clone()),
                        json!({
                            "cardIds": selected,
                            "minimum": effective_minimum,
                            "maximum": effective_maximum,
                        }),
                    );
                    return Ok(());
                }
                let mut values = Vec::new();
                for count in effective_minimum..=effective_maximum {
                    values.extend(Self::resolution_selections(
                        &objects,
                        count,
                        128 - values.len(),
                    ));
                    if values.len() >= 128 {
                        break;
                    }
                }
                if effect["sameLandType"].as_bool().unwrap_or(false) {
                    values.retain(|selection| {
                        let selected = selection
                            .iter()
                            .map(String::as_str)
                            .filter_map(|instance_id| self.card_outside_battlefield(instance_id))
                            .map(|card| {
                                basic_land_mana(&card.definition)
                                    .into_iter()
                                    .flatten()
                                    .collect::<BTreeSet<_>>()
                            })
                            .collect::<Vec<_>>();
                        selected.len() <= 1
                            || !selected
                                .iter()
                                .skip(1)
                                .fold(selected[0].clone(), |shared, land_types| {
                                    shared.intersection(land_types).cloned().collect()
                                })
                                .is_empty()
                    });
                }
                // `with different names`, `with total mana value 8 or less`:
                // a constraint over the whole selection, not one candidate.
                let mut effective_minimum = effective_minimum;
                if let Some(constraints) = effect.get("setConstraints") {
                    values.retain(|selection| self.selection_satisfies(selection, constraints));
                    // No legal selection of the required size: the choice
                    // is made as far as possible (CR 609.3), with the
                    // largest smaller selection the constraints admit.
                    if values.is_empty() {
                        for count in (0..effective_minimum).rev() {
                            values = Self::resolution_selections(&objects, count, 128)
                                .into_iter()
                                .filter(|selection| {
                                    self.selection_satisfies(selection, constraints)
                                })
                                .collect();
                            if !values.is_empty() {
                                effective_minimum = count;
                                break;
                            }
                        }
                    }
                }
                if effect["onePerControlledColor"].as_bool().unwrap_or(false) {
                    let choosing_player_index = self.player_index(&player_id)?;
                    let controlled_colors = self.state.players[choosing_player_index]
                        .battlefield
                        .iter()
                        .flat_map(|permanent| card_color_symbols(&permanent.definition))
                        .collect::<BTreeSet<_>>();
                    values.retain(|selection| {
                        let colors = selection
                            .iter()
                            .filter_map(|instance_id| self.card_outside_battlefield(instance_id))
                            .map(|card| card_color_symbols(&card.definition))
                            .collect::<Vec<_>>();
                        colors.len() == selection.len()
                            && cards_can_match_distinct_colors(&colors, &controlled_colors)
                    });
                }
                self.make_resolution_decision(
                    decision_id,
                    &player_id,
                    stack_object,
                    DecisionChoice::CardSelection {
                        decision_id: decision_id.to_string(),
                        candidate_card_instance_ids: objects,
                        minimum: effective_minimum,
                        maximum: effective_maximum,
                        prompt: Self::resolution_choice_prompt(decision_id, next_effect, false),
                    },
                    values,
                    decisions,
                    provider,
                )?;
                // The engine keeps the final authority over a set constraint:
                // a selection returned outside the offered previews is checked
                // as a whole before any effect reads it.
                if let Some(constraints) = effect.get("setConstraints") {
                    let selected = decisions
                        .get(decision_id)
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .filter_map(Value::as_str)
                                .map(ToOwned::to_owned)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if !self.selection_satisfies(&selected, constraints) {
                        return Err(EngineError::new(format!(
                            "resolution decision {decision_id} violates its set constraint"
                        )));
                    }
                }
            }
            Some("chooseOrder") => {
                let Some(decision_id) = effect["id"].as_str() else {
                    return Ok(());
                };
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let objects = self
                    .resolve_runtime_object_ids_for_effect(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )?
                    .ok_or_else(|| {
                        EngineError::new("ordering requires an available object collection")
                    })?;
                // This legacy menu enumerates orders. Never silently expose
                // only a prefix of the legal permutations. A future structured
                // order provider can remove this explicit resource bound.
                let mut permutations = 1usize;
                for count in 2..=objects.len() {
                    permutations = permutations
                        .checked_mul(count)
                        .filter(|permutations| *permutations <= 128)
                        .ok_or_else(|| {
                            EngineError::new(
                                "ordering exceeds the supported budget of 128 permutations",
                            )
                        })?;
                }
                let values = Self::resolution_orders(&objects, 128);
                self.make_resolution_decision(
                    decision_id,
                    &player_id,
                    stack_object,
                    DecisionChoice::CardOrder {
                        decision_id: decision_id.to_string(),
                        card_instance_ids: objects,
                        prompt: Self::resolution_choice_prompt(decision_id, next_effect, true),
                    },
                    values,
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

/// The five colors a color choice offers (CR 105.1), in the spelling the
/// shared color predicate (`colorContains`) reads.
const RESOLUTION_COLOR_NAMES: [&str; 5] = ["white", "blue", "black", "red", "green"];

/// The color symbols in WUBRG order, aligned with [`RESOLUTION_COLOR_NAMES`].
const COLOR_SYMBOLS: [&str; 5] = ["W", "U", "B", "R", "G"];

fn known_fields(effect: &Value, fields: &[&str]) -> bool {
    effect
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
}

/// The literal bounds of a `chooseColors`: `1 <= minimum <= maximum <= 5`
/// distinct colors. A computed bound is refused, never clamped.
fn color_count_bounds(effect: &Value) -> Option<(usize, usize)> {
    let minimum = usize::try_from(integer_value(&effect["minimum"])?).ok()?;
    let maximum = usize::try_from(integer_value(&effect["maximum"])?).ok()?;
    (1 <= minimum && minimum <= maximum && maximum <= COLOR_SYMBOLS.len())
        .then_some((minimum, maximum))
}

/// Every set of distinct color symbols whose size lies in the bounds, each
/// in WUBRG order, smaller sets first.
fn color_selections(minimum: usize, maximum: usize) -> Vec<Vec<String>> {
    let mut selections = (1_u32..(1 << COLOR_SYMBOLS.len()))
        .map(|mask| {
            COLOR_SYMBOLS
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, symbol)| (*symbol).to_string())
                .collect::<Vec<_>>()
        })
        .filter(|symbols| (minimum..=maximum).contains(&symbols.len()))
        .collect::<Vec<_>>();
    selections.sort_by_key(Vec::len);
    selections
}

fn color_name_of_symbol(symbol: &str) -> &'static str {
    COLOR_SYMBOLS
        .iter()
        .position(|candidate| *candidate == symbol)
        .map_or("colorless", |index| RESOLUTION_COLOR_NAMES[index])
}

/// The one text value a resolution choice recorded (`"red"` or `["red"]`).
fn resolution_choice_text(value: Option<&Value>) -> Option<&str> {
    let value = value?;
    value
        .as_str()
        .or_else(|| match value.as_array()?.as_slice() {
            [single] => single.as_str(),
            _ => None,
        })
}

impl GameEngine {
    /// Materializes the predicates that read a characteristic chosen earlier
    /// in this resolution (CR 608.2d) into the concrete predicate the shared
    /// object filters evaluate: `chosenColor` becomes `colorContains` of the
    /// recorded color, `chosenCardName` becomes `nameEquals` of the recorded
    /// name. A reference whose producer has not run is left unchanged, so no
    /// consumer ever reads a guessed value.
    pub(in crate::engine) fn materialize_resolution_choices<'a>(
        &self,
        effect: &'a Value,
        decisions: &BTreeMap<String, Value>,
    ) -> std::borrow::Cow<'a, Value> {
        fn reads_choice(value: &Value) -> bool {
            if matches!(value_kind(value), Some("chosenColor" | "chosenCardName")) {
                return true;
            }
            match value {
                Value::Object(values) => values.values().any(reads_choice),
                Value::Array(values) => values.iter().any(reads_choice),
                _ => false,
            }
        }
        fn materialize(value: &mut Value, decisions: &BTreeMap<String, Value>) {
            if value_kind(value) == Some("chosenColor") {
                let color = value["decisionId"]
                    .as_str()
                    .and_then(|id| resolution_choice_text(decisions.get(id)))
                    .filter(|color| RESOLUTION_COLOR_NAMES.contains(color))
                    .map(str::to_owned);
                if let Some(color) = color {
                    *value = json!({ "kind": "colorContains", "value": color });
                }
                return;
            }
            if value_kind(value) == Some("chosenCardName") {
                let name = value["decisionId"]
                    .as_str()
                    .and_then(|id| resolution_choice_text(decisions.get(id)))
                    .map(str::to_owned);
                if let Some(name) = name {
                    *value = json!({ "kind": "nameEquals", "value": name });
                }
                return;
            }
            match value {
                Value::Object(values) => values
                    .values_mut()
                    .for_each(|value| materialize(value, decisions)),
                Value::Array(values) => values
                    .iter_mut()
                    .for_each(|value| materialize(value, decisions)),
                _ => {}
            }
        }
        if !reads_choice(effect) {
            return std::borrow::Cow::Borrowed(effect);
        }
        let mut materialized = effect.clone();
        materialize(&mut materialized, decisions);
        std::borrow::Cow::Owned(materialized)
    }
}

/// A choice restricted to the candidates with the least or greatest current
/// power or toughness among the qualifying permanents, ties included (for
/// example bolster, CR 701.39a). It filters the candidate set; the player
/// still chooses among the tied candidates.
fn candidate_extremum_supported(extremum: &Value) -> bool {
    matches!(
        extremum["property"].as_str(),
        Some("power" | "toughness" | "manaValue")
    ) && matches!(extremum["direction"].as_str(), Some("least" | "greatest"))
}

/// `setConstraints`: the constraints a chosen set must satisfy as a whole
/// (`distinctValues` of names, powers or mana values; `totalAtMost` of one
/// numeric property with a literal bound). An unknown constraint refuses
/// the choice rather than being skipped.
fn set_constraints_supported(constraints: &Value) -> bool {
    constraints.as_array().is_some_and(|constraints| {
        !constraints.is_empty()
            && constraints
                .iter()
                .all(object_predicate_eval::set_constraint_supported)
    })
}

impl GameEngine {
    fn retain_candidate_extremum(&self, objects: Vec<String>, extremum: &Value) -> Vec<String> {
        let value_of = |instance_id: &String| {
            self.permanent_position(instance_id).map(|(player, index)| {
                let card = &self.state.players[player].battlefield[index];
                match extremum["property"].as_str() {
                    Some("power") => self.current_power(card),
                    Some("manaValue") => mana_value(&card.definition),
                    _ => self.current_toughness(card),
                }
            })
        };
        let values = objects.iter().filter_map(value_of).collect::<Vec<_>>();
        let target = if extremum["direction"].as_str() == Some("greatest") {
            values.iter().max()
        } else {
            values.iter().min()
        };
        let Some(target) = target.copied() else {
            return objects;
        };
        objects
            .into_iter()
            .filter(|instance_id| value_of(instance_id) == Some(target))
            .collect()
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "choices_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "choices_set_constraint_tests.rs"]
mod set_constraint_tests;
