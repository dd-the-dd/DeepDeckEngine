//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

// Collection planning and numeric-query preparation can reject before commit.
// The shared resolution visitor checkpoints the complete enclosing program.
pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    value_kind(value) == Some("destroyPermanent")
        && value_kind(&value["permanent"]) == Some("eachPermanent")
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("destroyPermanent") => {
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
                && (value_kind(effect) != Some("destroyPermanent")
                    || effect.get("cannotRegenerate").is_none_or(Value::is_boolean))
        }
        Some("destroyChosenPermanents") => {
            value_kind(&effect["objects"]) == Some("chosenTargets")
                && effect["objects"]["id"].is_string()
        }

        Some("sacrificePermanent") => {
            (matches!(
                value_kind(&effect["permanent"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["permanent"])
                || matches!(
                    value_kind(&effect["permanent"]),
                    Some("singleBoundObject" | "boundObject")
                ))
                && effect.get("bind").is_none_or(Value::is_string)
        }
        Some("sacrificePermanents") => {
            player_reference_supported(&effect["player"])
                && card_filter_supported(&effect["where"])
                && numeric_expression_supported(&effect["count"])
                && effect.get("bind").is_none_or(Value::is_string)
                && effect.get("bindPowerAs").is_none_or(Value::is_string)
                && effect
                    .get("minimumPowerAmongCandidates")
                    .is_none_or(Value::is_boolean)
                && effect
                    .get("maximumManaValueAmongCandidates")
                    .is_none_or(Value::is_boolean)
        }
        Some("sacrificePermanentsEachPlayer") => {
            matches!(effect["scope"].as_str(), Some("allPlayers" | "opponents"))
                && player_reference_supported(&effect["controller"])
                && card_filter_supported(&effect["where"])
                && (effect["all"].as_bool() == Some(true)
                    || integer_value(&effect["count"]).is_some_and(|count| count > 0))
                && effect
                    .get("candidateIdsDecision")
                    .is_none_or(Value::is_string)
        }

        Some("sacrificeAbilitySource") => true,
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_destruction_sacrifice_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_destruction_sacrifice_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_destruction_sacrifice_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("destroyPermanent") => {
                let mut destroyed_ids = Vec::new();
                if value_kind(&effect["permanent"]) == Some("chosenTargets") {
                    let decision_id = effect["permanent"]["id"].as_str().ok_or_else(|| {
                        EngineError::new("target collection has no declaration decision id")
                    })?;
                    let ordinal_prefix = format!("{decision_id}:");
                    let instance_ids = stack_object
                        .targets
                        .iter()
                        .filter(|(target_id, target)| {
                            (target_id.as_str() == decision_id
                                || target_id.starts_with(&ordinal_prefix))
                                && self.target_is_legal_for_stack_object(
                                    target_id,
                                    target,
                                    stack_object,
                                )
                        })
                        .filter_map(|(_, target)| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                            _ => None,
                        })
                        .collect();
                    destroyed_ids.extend(self.destroy_permanent_collection_from_stack(
                        instance_ids,
                        effect["cannotRegenerate"].as_bool().unwrap_or(false),
                        stack_object,
                    )?);
                } else if value_kind(&effect["permanent"]) == Some("eachPermanent") {
                    let prepared_filter = self.prepare_runtime_object_filter(
                        &effect["permanent"]["where"],
                        stack_object,
                        bindings,
                        decisions,
                    )?;
                    let mut excluded = effect["permanent"]
                        .get("exclude")
                        .and_then(|expression| {
                            self.resolve_object_ids(expression, bindings, decisions)
                        })
                        .unwrap_or_default()
                        .into_iter()
                        .collect::<BTreeSet<_>>();
                    if effect["permanent"]["excludeSource"]
                        .as_bool()
                        .unwrap_or(false)
                    {
                        excluded.insert(stack_object.card.instance_id.clone());
                    }
                    // The controller scope is a player population (`you`,
                    // `your opponents`, `players other than you`).
                    let controllers = effect["permanent"]
                        .get("player")
                        .map(|player| self.resolve_players(player, stack_object, bindings));
                    let instance_ids = self
                        .state
                        .players
                        .iter()
                        .flat_map(|player| player.battlefield.iter())
                        .filter(|permanent| {
                            !permanent.flags.get("phasedOut").copied().unwrap_or(false)
                        })
                        .filter(|permanent| !excluded.contains(&permanent.instance_id))
                        .filter(|permanent| {
                            controllers
                                .as_ref()
                                .is_none_or(|players| players.contains(&permanent.controller))
                        })
                        .filter(|permanent| {
                            self.runtime_object_filter_matches(
                                runtime_object_filter::ObjectFilterView::Permanent(permanent),
                                &prepared_filter,
                                stack_object,
                                bindings,
                                decisions,
                            )
                        })
                        .map(|permanent| permanent.instance_id.clone())
                        .collect::<Vec<_>>();
                    destroyed_ids.extend(self.destroy_permanent_collection_from_stack(
                        instance_ids,
                        effect["cannotRegenerate"].as_bool().unwrap_or(false),
                        stack_object,
                    )?);
                } else if value_kind(&effect["permanent"]) == Some("union") {
                    // Several declared target slots of one instruction are
                    // destroyed simultaneously; members that became illegal
                    // are unaffected and the rest still are (CR 608.2b).
                    let instance_ids = self
                        .resolve_runtime_object_ids_for_effect(
                            &effect["permanent"],
                            stack_object,
                            bindings,
                            decisions,
                        )?
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|instance_id| self.permanent_position(instance_id).is_some())
                        .collect();
                    destroyed_ids.extend(self.destroy_permanent_collection_from_stack(
                        instance_ids,
                        effect["cannotRegenerate"].as_bool().unwrap_or(false),
                        stack_object,
                    )?);
                } else if object_expression_supported(&effect["permanent"]) {
                    let instance_ids = self
                        .resolve_runtime_object_ids(
                            &effect["permanent"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .unwrap_or_default();
                    for instance_id in instance_ids {
                        if effect["cannotRegenerate"].as_bool().unwrap_or(false) {
                            self.state.rule_modifiers.retain(|modifier| {
                                !(value_kind(modifier) == Some("regenerationShield")
                                    && modifier["cardInstanceId"].as_str()
                                        == Some(instance_id.as_str()))
                            });
                        }
                        let was_on_battlefield = self.permanent_position(&instance_id).is_some();
                        self.destroy_permanent_from_stack(&instance_id, stack_object)?;
                        if was_on_battlefield && self.permanent_position(&instance_id).is_none() {
                            destroyed_ids.push(instance_id);
                        }
                    }
                } else if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                {
                    if effect["cannotRegenerate"].as_bool().unwrap_or(false) {
                        self.state.rule_modifiers.retain(|modifier| {
                            !(value_kind(modifier) == Some("regenerationShield")
                                && modifier["cardInstanceId"].as_str()
                                    == Some(instance_id.as_str()))
                        });
                    }
                    let was_on_battlefield = self.permanent_position(&instance_id).is_some();
                    self.destroy_permanent_from_stack(&instance_id, stack_object)?;
                    if was_on_battlefield && self.permanent_position(&instance_id).is_none() {
                        destroyed_ids.push(instance_id);
                    }
                }
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(binding.to_string(), RuntimeBinding::Objects(destroyed_ids));
                }
            }
            Some("sacrificePermanent") => {
                let instance_id = match value_kind(&effect["permanent"]) {
                    Some("self" | "abilitySource") => Some(stack_object.card.instance_id.clone()),
                    _ => self
                        .resolve_target(&effect["permanent"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        }),
                };
                let sacrificed = if let Some(instance_id) = instance_id {
                    self.sacrifice_permanent_for_effect(&instance_id, stack_object)?
                } else {
                    Vec::new()
                };
                if let Some(id) = effect["bind"].as_str() {
                    bindings.insert(id.to_string(), RuntimeBinding::Objects(sacrificed));
                }
            }
            Some("destroyChosenPermanents") => {
                let instance_ids = self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                for instance_id in instance_ids {
                    self.move_permanent_to_graveyard(&instance_id, "destroyed")?;
                }
            }

            Some("sacrificePermanents") => {
                if self.value_requires_initially_illegal_target_in_context(
                    &effect["player"],
                    stack_object,
                    bindings,
                    decisions,
                )? || self.value_requires_initially_illegal_target_in_context(
                    &effect["where"],
                    stack_object,
                    bindings,
                    decisions,
                )? || self.numeric_amount_requires_initially_illegal_target(
                    &effect["count"],
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    return Ok(());
                }
                let player_id = self
                    .resolve_player(&effect["player"], stack_object, bindings)
                    .ok_or_else(|| EngineError::new("unresolved sacrifice player"))?;
                self.player_index(&player_id)?;
                let count = self
                    .checked_runtime_numeric_expression(
                        &effect["count"],
                        stack_object,
                        bindings,
                        decisions,
                        "sacrificePermanents.count",
                    )?
                    .max(0) as usize;
                let filter = self.prepare_permanent_runtime_object_filter(
                    &effect["where"],
                    None,
                    stack_object,
                    bindings,
                    decisions,
                )?;
                if !self.effect_sacrifice_player_allowed(&player_id, stack_object) {
                    return Ok(());
                }
                let mut candidates = Vec::new();
                for card in self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| &player.battlefield)
                {
                    if card.controller != player_id
                        || !self.permanent_can_be_sacrificed(card)
                        || (effect["excludeSource"].as_bool() == Some(true)
                            && card.instance_id == stack_object.card.instance_id)
                    {
                        continue;
                    }
                    let candidate_filter = self.prepare_permanent_runtime_object_filter(
                        &filter,
                        Some(card),
                        stack_object,
                        bindings,
                        decisions,
                    )?;
                    if self.runtime_object_filter_matches(
                        runtime_object_filter::ObjectFilterView::Permanent(card),
                        &candidate_filter,
                        stack_object,
                        bindings,
                        decisions,
                    ) {
                        candidates.push(card.instance_id.clone());
                    }
                }
                if effect["minimumPowerAmongCandidates"].as_bool() == Some(true)
                    && let Some(minimum_power) = candidates
                        .iter()
                        .filter_map(|instance_id| self.permanent_position(instance_id))
                        .map(|(candidate_player, candidate_index)| {
                            self.current_power(
                                &self.state.players[candidate_player].battlefield[candidate_index],
                            )
                        })
                        .min()
                {
                    candidates.retain(|instance_id| {
                        self.permanent_position(instance_id).is_some_and(
                            |(candidate_player, candidate_index)| {
                                self.current_power(
                                    &self.state.players[candidate_player].battlefield
                                        [candidate_index],
                                ) == minimum_power
                            },
                        )
                    });
                }
                if effect["maximumManaValueAmongCandidates"].as_bool() == Some(true)
                    && let Some(maximum_mana_value) = candidates
                        .iter()
                        .filter_map(|instance_id| self.permanent_position(instance_id))
                        .map(|(candidate_player, candidate_index)| {
                            mana_value(
                                &self.state.players[candidate_player].battlefield[candidate_index]
                                    .definition,
                            )
                        })
                        .max()
                {
                    candidates.retain(|instance_id| {
                        self.permanent_position(instance_id).is_some_and(
                            |(candidate_player, candidate_index)| {
                                mana_value(
                                    &self.state.players[candidate_player].battlefield
                                        [candidate_index]
                                        .definition,
                                ) == maximum_mana_value
                            },
                        )
                    });
                }
                let count = count.min(candidates.len());
                if count == 0 {
                    if let Some(id) = effect["bind"].as_str() {
                        bindings.insert(id.to_owned(), RuntimeBinding::Objects(Vec::new()));
                    }
                    if let Some(id) = effect["bindPowerAs"].as_str() {
                        bindings.insert(id.to_owned(), RuntimeBinding::Integer(0));
                    }
                    return Ok(());
                }
                let values = Self::resolution_selections(&candidates, count, 128);
                let mut local_decisions = BTreeMap::new();
                self.make_resolution_decision(
                    "sacrificePermanents",
                    &player_id,
                    stack_object,
                    DecisionChoice::CardSelection {
                        decision_id: "sacrificePermanents".to_string(),
                        candidate_card_instance_ids: candidates,
                        minimum: count,
                        maximum: count,
                        prompt: format!("Choose {count} permanent(s) to sacrifice"),
                    },
                    values,
                    &mut local_decisions,
                    provider,
                )?;
                let selected = local_decisions["sacrificePermanents"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                if let Some(binding) = effect["bind"].as_str() {
                    bindings.insert(
                        binding.to_string(),
                        RuntimeBinding::Objects(selected.clone()),
                    );
                }
                if let Some(binding) = effect["bindPowerAs"].as_str() {
                    let power = selected
                        .iter()
                        .filter_map(|instance_id| self.permanent_position(instance_id))
                        .map(|(player_index, card_index)| {
                            self.current_power(
                                &self.state.players[player_index].battlefield[card_index],
                            )
                        })
                        .sum();
                    bindings.insert(binding.to_string(), RuntimeBinding::Integer(power));
                }
                self.sacrifice_effect_collection(selected, &[player_id], stack_object)?;
            }
            Some("sacrificePermanentsEachPlayer") => {
                let Some(source_controller) =
                    self.resolve_player(&effect["controller"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let scope = effect["scope"].as_str().unwrap_or("allPlayers");
                let count = integer_value(&effect["count"]).unwrap_or(1).max(0) as usize;
                let mut player_ids = self.player_iteration_ids(
                    &json!({"kind":"eachPlayer"}),
                    stack_object,
                    bindings,
                )?;
                if scope == "opponents" {
                    player_ids.retain(|id| id != &source_controller);
                }
                let mut all_selected = Vec::new();
                for player_id in player_ids.iter().cloned() {
                    if !self.effect_sacrifice_player_allowed(&player_id, stack_object) {
                        continue;
                    }
                    let player_index = self.player_index(&player_id)?;
                    let candidate_ids = effect["candidateIdsDecision"]
                        .as_str()
                        .and_then(|decision_id| stack_object.decisions.get(decision_id))
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<BTreeSet<_>>()
                        });
                    let candidates = self.state.players[player_index]
                        .battlefield
                        .iter()
                        .filter(|card| self.permanent_can_be_sacrificed(card))
                        .filter(|card| {
                            self.permanent_matches_effective_filter(card, &effect["where"])
                        })
                        .filter(|card| {
                            candidate_ids
                                .as_ref()
                                .is_none_or(|allowed| allowed.contains(card.instance_id.as_str()))
                        })
                        .map(|card| card.instance_id.clone())
                        .collect::<Vec<_>>();
                    let required = if effect["all"].as_bool() == Some(true) {
                        candidates.len()
                    } else {
                        count.min(candidates.len())
                    };
                    if required == 0 {
                        continue;
                    }
                    let values = Self::resolution_selections(&candidates, required, 128);
                    let mut local_decisions = BTreeMap::new();
                    self.make_resolution_decision(
                        "sacrificePermanentsEachPlayer",
                        &player_id,
                        stack_object,
                        DecisionChoice::CardSelection {
                            decision_id: "sacrificePermanentsEachPlayer".to_string(),
                            candidate_card_instance_ids: candidates,
                            minimum: required,
                            maximum: required,
                            prompt: format!("Choose {required} permanent(s) to sacrifice"),
                        },
                        values,
                        &mut local_decisions,
                        provider,
                    )?;
                    let selected = local_decisions["sacrificePermanentsEachPlayer"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>();
                    all_selected.extend(selected);
                }
                self.sacrifice_effect_collection(all_selected, &player_ids, stack_object)?;
            }

            Some("sacrificeAbilitySource") => {
                if self
                    .permanent_position(&stack_object.card.instance_id)
                    .is_some()
                {
                    self.move_permanent_to_graveyard(&stack_object.card.instance_id, "sacrificed")?;
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
