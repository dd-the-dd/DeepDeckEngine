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
        }
        Some("chooseNumber") => {
            effect["id"].is_string()
                && player_reference_supported(&effect["player"])
                && numeric_expression_supported(&effect["minimum"])
                && numeric_expression_supported(&effect["maximum"])
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
                let candidates = self
                    .state
                    .players
                    .iter()
                    .filter(|player| !player.has_lost)
                    .map(|player| player.id.clone())
                    .collect::<Vec<_>>();
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
                let mut values = Vec::new();
                for count in minimum..=maximum {
                    values.extend(Self::resolution_selections(
                        &candidates,
                        count,
                        128 - values.len(),
                    ));
                }
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

/// A choice restricted to the candidates with the least or greatest current
/// power or toughness among the qualifying permanents, ties included (for
/// example bolster, CR 701.39a). It filters the candidate set; the player
/// still chooses among the tied candidates.
fn candidate_extremum_supported(extremum: &Value) -> bool {
    matches!(extremum["property"].as_str(), Some("power" | "toughness"))
        && matches!(extremum["direction"].as_str(), Some("least" | "greatest"))
}

impl GameEngine {
    fn retain_candidate_extremum(&self, objects: Vec<String>, extremum: &Value) -> Vec<String> {
        let value_of = |instance_id: &String| {
            self.permanent_position(instance_id).map(|(player, index)| {
                let card = &self.state.players[player].battlefield[index];
                if extremum["property"].as_str() == Some("power") {
                    self.current_power(card)
                } else {
                    self.current_toughness(card)
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
