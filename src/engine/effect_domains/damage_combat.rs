//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("fightPermanents") => {
            (target_reference_supported(&effect["first"])
                || matches!(value_kind(&effect["first"]), Some("self" | "abilitySource")))
                && (target_reference_supported(&effect["second"])
                    || matches!(
                        value_kind(&effect["second"]),
                        Some("self" | "abilitySource")
                    ))
                && effect
                    .get("bindExcessToSecondAs")
                    .is_none_or(Value::is_string)
        }
        Some("tapPermanent") => {
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
        Some("tapPermanents") => {
            card_filter_supported(&effect["where"])
                && effect
                    .get("excludedController")
                    .is_none_or(player_reference_supported)
        }
        Some("untapPermanent") => {
            entry_observation::observed_object_reference_supported(&effect["permanent"])
                || target_reference_supported(&effect["permanent"])
                || matches!(
                    value_kind(&effect["permanent"]),
                    Some("self" | "abilitySource")
                )
        }
        Some("untapPermanents") => object_expression_supported(&effect["objects"]),
        Some("untapPermanentsMatching") => {
            player_reference_supported(&effect["player"])
                && card_filter_supported(&effect["where"])
                && effect.get("excludeSource").is_none_or(Value::is_boolean)
        }
        Some("untapAttackingPermanents" | "addCombatPhase") => true,
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_damage_combat_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_damage_combat_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_damage_combat_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("dealDamage") => {
                if self.execute_player_set_damage(effect, stack_object, bindings, decisions)? {
                    return Ok(());
                }
                let amount = self
                    .runtime_numeric_expression(
                        &effect["amount"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0);
                let damage_source_id = effect
                    .get("source")
                    .and_then(|source| self.resolve_target(source, stack_object, bindings))
                    .and_then(|source| match source {
                        TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                            Some(instance_id)
                        }
                        TargetRef::Player { .. } | TargetRef::StackObject { .. } => None,
                    })
                    .unwrap_or_else(|| stack_object.card.instance_id.clone());
                // A resolving spell has left the stack; its characteristics
                // remain those of the spell for replacement effects that
                // examine the damage source (CR 120.3, 609.7).
                let spell_profile = (damage_source_id == stack_object.card.instance_id
                    && stack_object_is_spell(stack_object))
                .then(|| self.resolve_damage_source_lki(stack_object).ok())
                .flatten();
                if value_kind(&effect["recipient"]) == Some("eachPlayer") {
                    let player_ids = self
                        .state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>();
                    for player_id in player_ids {
                        self.deal_damage_amount_with_source_profile(
                            TargetRef::Player { player_id },
                            amount,
                            &damage_source_id,
                            false,
                            QuantityOrigin::Effect,
                            Some(provider),
                            spell_profile.as_ref(),
                        )?;
                    }
                } else if value_kind(&effect["recipient"]) == Some("eachPermanent") {
                    let controller_reference = effect["recipient"].get("player");
                    let targets = self
                        .state
                        .players
                        .iter()
                        .flat_map(|player| player.battlefield.iter())
                        .filter(|card| {
                            let controller_matches = controller_reference.is_none_or(|reference| {
                                match value_kind(reference) {
                                    Some("opponentsOf") => {
                                        card.controller != stack_object.controller
                                    }
                                    Some("abilityController") => {
                                        card.controller == stack_object.controller
                                    }
                                    _ => true,
                                }
                            });
                            controller_matches
                                && matches_card_filter(
                                    &card.definition,
                                    &effect["recipient"]["where"],
                                )
                        })
                        .map(|card| TargetRef::Permanent {
                            instance_id: card.instance_id.clone(),
                        })
                        .collect::<Vec<_>>();
                    for target in targets {
                        self.deal_damage_amount_with_source_profile(
                            target,
                            amount,
                            &damage_source_id,
                            false,
                            QuantityOrigin::Effect,
                            Some(provider),
                            spell_profile.as_ref(),
                        )?;
                    }
                } else if let Some(target) =
                    self.resolve_target(&effect["recipient"], stack_object, bindings)
                {
                    let lethal_threshold = match &target {
                        TargetRef::Permanent { instance_id } => self
                            .permanent_position(instance_id)
                            .map(|(player_index, card_index)| {
                                let permanent =
                                    &self.state.players[player_index].battlefield[card_index];
                                self.current_toughness(permanent)
                                    .saturating_sub(permanent.damage_marked)
                                    .max(0)
                            }),
                        _ => None,
                    };
                    let recipient_id = match &target {
                        TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id } => {
                            Some(instance_id.clone())
                        }
                        _ => None,
                    };
                    let dealt = self.deal_damage_amount_with_source_profile(
                        target,
                        amount,
                        &damage_source_id,
                        false,
                        QuantityOrigin::Effect,
                        Some(provider),
                        spell_profile.as_ref(),
                    )?;
                    if let (Some(binding), Some(lethal)) =
                        (effect["bindExcessAs"].as_str(), lethal_threshold)
                    {
                        bindings.insert(
                            binding.to_string(),
                            RuntimeBinding::Integer(dealt.saturating_sub(lethal).max(0)),
                        );
                    }
                    if let (Some(binding), Some(instance_id)) =
                        (effect["bindRecipientAs"].as_str(), recipient_id)
                    {
                        bindings.insert(
                            binding.to_string(),
                            RuntimeBinding::Objects(vec![instance_id]),
                        );
                    }
                } else if let Some(player_id) =
                    self.resolve_player(&effect["recipient"], stack_object, bindings)
                {
                    self.deal_damage_from_effect(
                        TargetRef::Player { player_id },
                        amount,
                        &damage_source_id,
                        provider,
                    )?;
                }
            }
            Some("dealDividedDamage") => {
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
                if targets.len() == division.len() && division.iter().all(|amount| *amount > 0) {
                    for (target, amount) in targets.into_iter().zip(division) {
                        self.deal_damage_from_effect(
                            target,
                            amount,
                            &stack_object.card.instance_id,
                            provider,
                        )?;
                    }
                }
            }

            Some("fightPermanents") => {
                let Some(TargetRef::Permanent {
                    instance_id: first_id,
                }) = self.resolve_target(&effect["first"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(TargetRef::Permanent {
                    instance_id: second_id,
                }) = self.resolve_target(&effect["second"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((first_player, first_index)) = self.permanent_position(&first_id) else {
                    return Ok(());
                };
                let Some((second_player, second_index)) = self.permanent_position(&second_id)
                else {
                    return Ok(());
                };
                let first_power =
                    self.current_power(&self.state.players[first_player].battlefield[first_index]);
                let second_power = self
                    .current_power(&self.state.players[second_player].battlefield[second_index]);
                let second_remaining_toughness = self
                    .current_toughness(&self.state.players[second_player].battlefield[second_index])
                    .saturating_sub(
                        self.state.players[second_player].battlefield[second_index].damage_marked,
                    )
                    .max(0);
                if let Some(binding) = effect["bindExcessToSecondAs"].as_str() {
                    bindings.insert(
                        binding.to_string(),
                        RuntimeBinding::Integer(
                            first_power
                                .saturating_sub(second_remaining_toughness)
                                .max(0),
                        ),
                    );
                }
                self.deal_damage_from_effect(
                    TargetRef::Permanent {
                        instance_id: second_id.clone(),
                    },
                    first_power,
                    &first_id,
                    provider,
                )?;
                self.deal_damage_from_effect(
                    TargetRef::Permanent {
                        instance_id: first_id,
                    },
                    second_power,
                    &second_id,
                    provider,
                )?;
            }
            Some("tapPermanent") => {
                // The ability source is not a target reference; `tap this
                // permanent` names the resolving ability's own source.
                let instance_ids = if matches!(
                    value_kind(&effect["permanent"]),
                    Some("self" | "abilitySource")
                ) {
                    vec![stack_object.card.instance_id.clone()]
                } else if value_kind(&effect["permanent"]) == Some("chosenTargets") {
                    self.resolve_runtime_object_ids(
                        &effect["permanent"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default()
                } else {
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                            _ => None,
                        })
                        .unwrap_or_default()
                };
                let mut tapped_sources = Vec::new();
                for instance_id in instance_ids {
                    if self.permanent_position(&instance_id).is_some()
                        && let Some(source) = self.tap_permanent(&instance_id)?
                    {
                        tapped_sources.push(source);
                    }
                }
                self.enqueue_permanents_tapped_triggers(&tapped_sources);
            }
            Some("tapPermanents") => {
                let excluded_controller = effect
                    .get("excludedController")
                    .and_then(|player| self.resolve_player(player, stack_object, bindings));
                let instance_ids = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| {
                        excluded_controller
                            .as_ref()
                            .is_none_or(|player_id| permanent.controller != *player_id)
                            && matches_card_filter(&permanent.definition, &effect["where"])
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                let mut tapped_sources = Vec::new();
                for instance_id in instance_ids {
                    if let Some(source) = self.tap_permanent(&instance_id)? {
                        tapped_sources.push(source);
                    }
                }
                self.enqueue_permanents_tapped_triggers(&tapped_sources);
            }
            Some("untapPermanent") => {
                let instance_id = match self.prepare_permanent_action_subject(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    permanent_subject::PermanentSubjectResolution::Unavailable => {
                        return Ok(());
                    }
                    permanent_subject::PermanentSubjectResolution::Resolved(ids) => {
                        ids.into_iter().next()
                    }
                    permanent_subject::PermanentSubjectResolution::Historical => {
                        if value_kind(&effect["permanent"]) == Some("observedEventObject") {
                            self.resolve_observed_event_permanent(
                                &effect["permanent"],
                                stack_object,
                            )?
                        } else if matches!(
                            value_kind(&effect["permanent"]),
                            Some("self" | "abilitySource")
                        ) {
                            Some(stack_object.card.instance_id.clone())
                        } else {
                            self.resolve_target(&effect["permanent"], stack_object, bindings)
                                .and_then(|target| match target {
                                    TargetRef::Permanent { instance_id } => Some(instance_id),
                                    _ => None,
                                })
                        }
                    }
                };
                if let Some(instance_id) = instance_id
                    && self.attempt_untap_permanent(&instance_id)
                        == untap_attempt::UntapAttemptOutcome::Untapped
                {
                    self.record_event(
                        "permanentUntapped",
                        Some(stack_object.controller.clone()),
                        Some(instance_id),
                        json!({ "reason": "effect" }),
                    );
                }
            }
            Some("untapPermanents") => {
                let mut attempted = BTreeSet::new();
                for instance_id in self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default()
                {
                    if attempted.insert(instance_id.clone())
                        && self.attempt_untap_permanent(&instance_id)
                            == untap_attempt::UntapAttemptOutcome::Untapped
                    {
                        self.record_event(
                            "permanentUntapped",
                            Some(stack_object.controller.clone()),
                            Some(instance_id),
                            json!({ "reason": "effect" }),
                        );
                    }
                }
            }
            Some("untapPermanentsMatching") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let instance_ids = self.state.players[player_index]
                    .battlefield
                    .iter()
                    .filter(|permanent| {
                        (!effect["excludeSource"].as_bool().unwrap_or(false)
                            || permanent.instance_id != stack_object.card.instance_id)
                            && matches_card_filter(&permanent.definition, &effect["where"])
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                for instance_id in instance_ids {
                    if self.attempt_untap_permanent(&instance_id)
                        == untap_attempt::UntapAttemptOutcome::Untapped
                    {
                        self.record_event(
                            "permanentUntapped",
                            Some(player_id.clone()),
                            Some(instance_id),
                            json!({ "reason": "effect" }),
                        );
                    }
                }
            }
            Some("untapAttackingPermanents") => {
                let instance_ids = self
                    .state
                    .combat
                    .attackers
                    .iter()
                    .map(|attacker| attacker.attacker_id.clone())
                    .collect::<Vec<_>>();
                for instance_id in instance_ids {
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    {
                        self.state.players[player_index].battlefield[card_index].tapped = false;
                        self.record_event(
                            "permanentUntapped",
                            Some(stack_object.controller.clone()),
                            Some(instance_id),
                            json!({ "reason": "effect" }),
                        );
                    }
                }
            }
            Some("addCombatPhase") => {
                self.state.rule_modifiers.push(json!({
                    "kind": "additionalCombat",
                    "playerId": stack_object.controller,
                    "landCreaturesOnly": false,
                    "turn": self.state.turn_number,
                }));
            }
            Some("chosenPermanentsDealPowerDamage") => {
                let source_ids = self
                    .resolve_runtime_object_ids(
                        &effect["sources"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                let Some(TargetRef::Permanent {
                    instance_id: recipient_id,
                }) = self.resolve_target(&effect["recipient"], stack_object, bindings)
                else {
                    return Ok(());
                };
                for source_id in source_ids {
                    let Some((player_index, card_index)) = self.permanent_position(&source_id)
                    else {
                        continue;
                    };
                    let power = self
                        .current_power(&self.state.players[player_index].battlefield[card_index]);
                    self.deal_damage_from_effect(
                        TargetRef::Permanent {
                            instance_id: recipient_id.clone(),
                        },
                        power,
                        &source_id,
                        provider,
                    )?;
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
