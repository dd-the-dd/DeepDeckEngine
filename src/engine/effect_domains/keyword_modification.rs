//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("removeKeywordsFromPermanents") => {
            static_object_selector_supported(&effect["objects"])
                && effect["keywords"].as_array().is_some_and(|keywords| {
                    !keywords.is_empty()
                        && keywords.iter().all(|keyword| {
                            keyword.as_str().is_some_and(|keyword| {
                                keyword_kind_supported(&json!({ "kind": keyword }))
                            })
                        })
                })
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }

        Some("installLoseAllAbilities") => {
            super::characteristic_object_supported(&effect["object"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("grantAbility") => {
            let permanent_trigger_supported = (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && value_kind(&effect["duration"]) == Some("permanent")
                && (triggered_rule_supported(&effect["ability"])
                    || activated_rule_supported(&effect["ability"])
                    || mana_rule_supported(&effect["ability"])
                    || value_kind(&effect["ability"]) == Some("staticAbility")
                        && rule_supported(&effect["ability"]));
            let flashback_supported = target_reference_supported(&effect["object"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && value_kind(&effect["ability"]) == Some("flashback")
                && value_kind(&effect["ability"]["cost"]) == Some("manaCostOf")
                && value_kind(&effect["ability"]["cost"]["card"]) == Some("abilitySource");
            let temporary_trigger_supported = target_reference_supported(&effect["object"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && triggered_rule_supported(&effect["ability"]);
            let temporary_static_supported = matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]);
            let temporary_static_supported = temporary_static_supported
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && value_kind(&effect["ability"]) == Some("staticAbility")
                && rule_supported(&effect["ability"]);
            // A temporary granted rule is appended to each affected permanent's
            // rules and removed at cleanup, whatever its kind (CR 113.1, 611.2c).
            let temporary_collection_supported = value_kind(&effect["object"])
                == Some("eachPermanent")
                && effect["object"]
                    .get("player")
                    .is_none_or(player_reference_supported)
                && card_filter_supported(&effect["object"]["where"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && (triggered_rule_supported(&effect["ability"])
                    || activated_rule_supported(&effect["ability"])
                    || mana_rule_supported(&effect["ability"]));
            let temporary_single_supported = (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && (activated_rule_supported(&effect["ability"])
                    || mana_rule_supported(&effect["ability"]));
            permanent_trigger_supported
                || flashback_supported
                || temporary_trigger_supported
                || temporary_static_supported
                || temporary_collection_supported
                || temporary_single_supported
        }
        Some("replaceAbility") => {
            matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) && value_kind(&effect["duration"]) == Some("permanent")
                && rule_supported(&effect["from"])
                && rule_supported(&effect["to"])
        }
        Some("grantProtection") => {
            target_reference_supported(&effect["object"])
                && effect["from"].as_array().is_some_and(|qualities| {
                    !qualities.is_empty() && qualities.iter().all(Value::is_string)
                })
                && (value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                    || value_kind(&effect["duration"]) == Some("untilNextTurn")
                        && player_reference_supported(&effect["duration"]["player"]))
        }
        Some("grantPlayerProtection") => {
            player_reference_supported(&effect["player"])
                && effect["from"].as_array().is_some_and(|qualities| {
                    qualities.len() == 1 && qualities[0].as_str() == Some("everything")
                })
                && value_kind(&effect["duration"]) == Some("untilNextTurn")
                && player_reference_supported(&effect["duration"]["player"])
        }
        Some("grantKeyword") => {
            let supported_keyword = if effect["keyword"].as_str() == Some("annihilator") {
                integer_value(&effect["quantity"]).is_some_and(|quantity| quantity > 0)
            } else {
                effect["keyword"]
                    .as_str()
                    .is_some_and(|keyword| keyword_kind_supported(&json!({ "kind": keyword })))
            };
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || entry_observation::observed_object_reference_supported(&effect["object"])
                || target_reference_supported(&effect["object"])
                || object_expression_supported(&effect["object"])
                || (value_kind(&effect["object"]) == Some("eachPermanent")
                    && card_filter_supported(&effect["object"]["where"])))
                && supported_keyword
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("untilEndOfCurrentTurn" | "untilNextTurn" | "permanent")
                )
                && (value_kind(&effect["duration"]) != Some("untilNextTurn")
                    || player_reference_supported(&effect["duration"]["player"]))
        }

        Some("grantSourceOtherAbilities") => target_reference_supported(&effect["object"]),
        Some("loseAllAbilitiesPermanently") => {
            target_reference_supported(&effect["object"])
                || matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                )
        }
        Some("grantKeywordToTargets") => {
            value_kind(&effect["objects"]) == Some("chosenTargets")
                && effect["objects"]["id"].is_string()
                && effect["keyword"]
                    .as_str()
                    .is_some_and(|keyword| keyword_kind_supported(&json!({ "kind": keyword })))
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("grantKeywordToAttackingPermanents") => {
            player_reference_supported(&effect["player"])
                && permanent_filter_supported(&effect["where"])
                && integer_value(&effect["minimumPower"]).is_some_and(|power| power >= 0)
                && effect["keyword"]
                    .as_str()
                    .is_some_and(|keyword| keyword_kind_supported(&json!({ "kind": keyword })))
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("grantKeywordToTriggeringAttackers") => {
            effect["keyword"]
                .as_str()
                .is_some_and(|keyword| keyword_kind_supported(&json!({ "kind": keyword })))
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_keyword_modification_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_keyword_modification_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_keyword_modification_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("removeKeywordsFromPermanents") => {
                let keywords = effect["keywords"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                let targets = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|permanent| {
                        Self::static_selector_matches(
                            &stack_object.card,
                            &effect["objects"],
                            permanent,
                        )
                    })
                    .map(|permanent| permanent.instance_id.clone())
                    .collect::<Vec<_>>();
                for instance_id in targets {
                    for keyword in &keywords {
                        self.state.rule_modifiers.push(json!({
                            "kind": "temporaryKeywordLoss",
                            "cardInstanceId": instance_id,
                            "keyword": keyword,
                            "expiresAfterTurn": self.state.turn_number,
                        }));
                    }
                }
            }
            Some("installLoseAllAbilities") => {
                // CR 611.2c: the affected set is locked in at resolution.
                for instance_id in self.characteristic_object_ids(
                    &effect["object"],
                    stack_object,
                    bindings,
                    decisions,
                ) {
                    self.state.rule_modifiers.push(json!({
                        "kind": "loseAllAbilities",
                        "cardInstanceId": instance_id,
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("grantAbility") => {
                if value_kind(&effect["ability"]) == Some("flashback") {
                    if let Some(TargetRef::Card { instance_id }) =
                        self.resolve_target(&effect["object"], stack_object, bindings)
                        && let Some(card) = self.card_outside_battlefield(&instance_id).cloned()
                    {
                        let permission = json!({
                            "action": "cast",
                            "alternativeManaCost": card.definition.mana_cost,
                            "cardInstanceId": instance_id,
                            "exileOnLeaveStack": true,
                            "expiresAfterTurn": self.state.turn_number,
                            "normalCostsApply": true,
                            "normalTimingApplies": true,
                            "playerId": stack_object.controller,
                            "sourceCardInstanceId": stack_object.card.instance_id,
                        });
                        self.state.permissions.push(permission.clone());
                        self.record_event(
                            "flashbackPermissionGranted",
                            Some(stack_object.controller.clone()),
                            Some(card.instance_id),
                            permission,
                        );
                    }
                } else if value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn") {
                    let instance_ids = if matches!(
                        value_kind(&effect["object"]),
                        Some("self" | "abilitySource")
                    ) {
                        vec![stack_object.card.instance_id.clone()]
                    } else if value_kind(&effect["object"]) == Some("eachPermanent") {
                        // The affected set is fixed on resolution (CR 611.2c).
                        let selected_players = effect["object"]
                            .get("player")
                            .map(|player| self.resolve_players(player, stack_object, bindings));
                        self.state
                            .players
                            .iter()
                            .flat_map(|player| player.battlefield.iter())
                            .filter(|permanent| {
                                (!effect["object"]["excludeSource"].as_bool().unwrap_or(false)
                                    || permanent.instance_id != stack_object.card.instance_id)
                                    && selected_players.as_ref().is_none_or(|players| {
                                        players.contains(&permanent.controller)
                                    })
                                    && self.permanent_matches_effective_filter(
                                        permanent,
                                        &effect["object"]["where"],
                                    )
                            })
                            .map(|permanent| permanent.instance_id.clone())
                            .collect()
                    } else {
                        self.resolve_target(&effect["object"], stack_object, bindings)
                            .and_then(|target| match target {
                                TargetRef::Permanent { instance_id } => Some(instance_id),
                                _ => None,
                            })
                            .into_iter()
                            .collect()
                    };
                    for instance_id in instance_ids {
                        let Some((player_index, card_index)) =
                            self.permanent_position(&instance_id)
                        else {
                            continue;
                        };
                        let mut ability = effect["ability"].clone();
                        ability["temporaryUntilTurn"] = Value::from(self.state.turn_number);
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
                ) && let Some((player_index, card_index)) =
                    self.permanent_position(&stack_object.card.instance_id)
                {
                    let ability = effect["ability"].clone();
                    self.state.players[player_index].battlefield[card_index]
                        .definition
                        .printed_rules_mut()
                        .push(ability.clone());
                    self.record_event(
                        "abilityGranted",
                        Some(stack_object.controller.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "ability": ability, "duration": effect["duration"] }),
                    );
                }
            }
            Some("replaceAbility") => {
                if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) && let Some((player_index, card_index)) =
                    self.permanent_position(&stack_object.card.instance_id)
                {
                    let from = &effect["from"];
                    let to = effect["to"].clone();
                    let permanent = &mut self.state.players[player_index].battlefield[card_index];
                    if let Some(rule_index) = permanent
                        .definition
                        .printed_rules()
                        .iter()
                        .position(|rule| rule == from)
                    {
                        permanent.definition.printed_rules_mut()[rule_index] = to.clone();
                        self.record_event(
                            "abilityReplaced",
                            Some(stack_object.controller.clone()),
                            Some(stack_object.card.instance_id.clone()),
                            json!({
                                "from": from,
                                "to": to,
                                "duration": effect["duration"],
                            }),
                        );
                    }
                }
            }
            Some("grantProtection") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let qualities = effect["from"].as_array().cloned().unwrap_or_default();
                let mut modifier = json!({
                    "kind": "temporaryProtection",
                    "cardInstanceId": instance_id,
                    "from": qualities,
                });
                if value_kind(&effect["duration"]) == Some("untilNextTurn") {
                    if let Some(player_id) =
                        self.resolve_player(&effect["duration"]["player"], stack_object, bindings)
                    {
                        modifier["expiresAtTurnStart"] = Value::String(player_id);
                    }
                } else {
                    modifier["expiresAfterTurn"] = Value::from(self.state.turn_number);
                }
                self.state.rule_modifiers.push(modifier);
            }
            Some("grantPlayerProtection") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some(expiry_player_id) =
                    self.resolve_player(&effect["duration"]["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "protectionFromEverything",
                    "playerId": player_id,
                    "expiresAtTurnStart": expiry_player_id,
                }));
            }

            Some("loseAllAbilitiesPermanently") => {
                let instance_id = if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    Some(stack_object.card.instance_id.clone())
                } else {
                    self.resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        })
                };
                if let Some(instance_id) = instance_id
                    && let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                {
                    self.state.players[player_index].battlefield[card_index]
                        .definition
                        .printed_rules_mut()
                        .push(json!({
                            "kind": "staticAbility",
                            "source": { "kind": "self" },
                            "activeWhile": {
                                "kind": "inZone",
                                "object": { "kind": "self" },
                                "zone": { "kind": "battlefield" },
                            },
                            "modifiers": [{
                                "kind": "loseAllAbilities",
                                "objects": { "kind": "self" },
                            }],
                        }));
                }
            }
            Some("grantSourceOtherAbilities") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let current_ability = stack_object.ability_rule.as_ref();
                let granted = stack_object
                    .card
                    .definition
                    .printed_rules()
                    .iter()
                    .filter(|rule| current_ability != Some(*rule))
                    .cloned()
                    .collect::<Vec<_>>();
                self.state.players[player_index].battlefield[card_index]
                    .definition
                    .printed_rules_mut()
                    .extend(granted);
            }
            Some("grantKeyword") => {
                let selected_players = effect["object"]
                    .get("player")
                    .map(|player| self.resolve_players(player, stack_object, bindings));
                let chosen_type = (value_kind(&effect["object"]["where"])
                    == Some("chosenCreatureType"))
                .then(|| {
                    effect["object"]["where"]["decisionId"]
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
                        .map(ToOwned::to_owned)
                })
                .flatten();
                let instance_ids = match self.prepare_permanent_action_subject(
                    effect,
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    permanent_subject::PermanentSubjectResolution::Unavailable => {
                        return Ok(());
                    }
                    permanent_subject::PermanentSubjectResolution::Resolved(ids) => ids,
                    permanent_subject::PermanentSubjectResolution::Historical => {
                        if matches!(
                            value_kind(&effect["object"]),
                            Some("self" | "abilitySource")
                        ) {
                            vec![stack_object.card.instance_id.clone()]
                        } else if value_kind(&effect["object"]) == Some("eachPermanent") {
                            self.state
                                .players
                                .iter()
                                .flat_map(|player| player.battlefield.iter())
                                .filter(|permanent| {
                                    (!effect["object"]["excludeSource"].as_bool().unwrap_or(false)
                                        || permanent.instance_id != stack_object.card.instance_id)
                                        && selected_players.as_ref().is_none_or(|players| {
                                            players.contains(&permanent.controller)
                                        })
                                        && chosen_type.as_ref().map_or_else(
                                            || {
                                                self.permanent_matches_effective_filter(
                                                    permanent,
                                                    &effect["object"]["where"],
                                                )
                                            },
                                            |creature_type| {
                                                self.permanent_has_keyword(permanent, "changeling")
                                                    || type_line_contains(
                                                        &permanent.definition.type_line,
                                                        creature_type,
                                                    )
                                            },
                                        )
                                })
                                .map(|permanent| permanent.instance_id.clone())
                                .collect::<Vec<_>>()
                        } else if object_expression_supported(&effect["object"]) {
                            self.resolve_runtime_object_ids(
                                &effect["object"],
                                stack_object,
                                bindings,
                                decisions,
                            )
                            .unwrap_or_default()
                        } else {
                            self.resolve_target(&effect["object"], stack_object, bindings)
                                .and_then(|target| match target {
                                    TargetRef::Permanent { instance_id }
                                    | TargetRef::Card { instance_id } => Some(vec![instance_id]),
                                    _ => None,
                                })
                                .unwrap_or_default()
                        }
                    }
                };
                let keyword = effect["keyword"].as_str().unwrap_or_default();
                let temporary = value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn");
                let expires_before_turn =
                    self.permanent_action_expiry_turn(&effect["duration"], stack_object, bindings)?;
                for instance_id in instance_ids {
                    if let Some(expires_before_turn) = expires_before_turn {
                        self.state.rule_modifiers.push(json!({
                            "kind": "temporaryKeywordUntilTurn",
                            "cardInstanceId": instance_id,
                            "keyword": keyword,
                            "expiresBeforeTurn": expires_before_turn,
                        }));
                    } else if let Some((player_index, card_index)) =
                        self.permanent_position(&instance_id)
                    {
                        let mut rule = json!({
                            "kind": "keywordAbility",
                            "source": { "kind": "self" },
                            "ability": { "kind": keyword },
                        });
                        if let Some(quantity) = effect.get("quantity") {
                            rule["ability"]["quantity"] = quantity.clone();
                        }
                        if temporary {
                            rule["temporaryUntilTurn"] = Value::from(self.state.turn_number);
                        }
                        self.state.players[player_index].battlefield[card_index]
                            .definition
                            .printed_rules_mut()
                            .push(rule);
                    }
                }
            }
            Some("grantKeywordToTargets") => {
                let keyword = effect["keyword"].as_str().unwrap_or_default();
                let instance_ids = self
                    .resolve_runtime_object_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default();
                for instance_id in instance_ids {
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    {
                        self.state.players[player_index].battlefield[card_index]
                            .definition
                            .printed_rules_mut()
                            .push(json!({
                                "kind": "keywordAbility",
                                "source": { "kind": "self" },
                                "ability": { "kind": keyword },
                                "temporaryUntilTurn": self.state.turn_number,
                            }));
                    }
                }
            }
            Some("grantKeywordToAttackingPermanents") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let minimum_power = integer_value(&effect["minimumPower"])
                    .and_then(|power| i32::try_from(power).ok())
                    .unwrap_or(0);
                let instance_ids = self
                    .state
                    .combat
                    .attackers
                    .iter()
                    .filter_map(|attacker| {
                        self.permanent_position(&attacker.attacker_id).and_then(
                            |(player_index, card_index)| {
                                let permanent =
                                    &self.state.players[player_index].battlefield[card_index];
                                (permanent.controller == player_id
                                    && matches_permanent_filter(permanent, &effect["where"])
                                    && self.current_power(permanent) >= minimum_power)
                                    .then(|| permanent.instance_id.clone())
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                let keyword = effect["keyword"].as_str().unwrap_or_default();
                for instance_id in instance_ids {
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    {
                        self.state.players[player_index].battlefield[card_index]
                            .definition
                            .printed_rules_mut()
                            .push(json!({
                                "kind": "keywordAbility",
                                "source": { "kind": "self" },
                                "ability": { "kind": keyword },
                                "temporaryUntilTurn": self.state.turn_number,
                            }));
                    }
                }
            }
            Some("grantKeywordToTriggeringAttackers") => {
                let keyword = effect["keyword"].as_str().unwrap_or_default();
                let instance_ids = stack_object.decisions["triggeringAttackerIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                for instance_id in instance_ids {
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    {
                        self.state.players[player_index].battlefield[card_index]
                            .definition
                            .printed_rules_mut()
                            .push(json!({
                                "kind": "keywordAbility",
                                "source": { "kind": "self" },
                                "ability": { "kind": keyword },
                                "temporaryUntilTurn": self.state.turn_number,
                            }));
                    }
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
