//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("modifyPowerToughness") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || entry_observation::observed_object_reference_supported(&effect["object"])
                || target_reference_supported(&effect["object"])
                || (matches!(
                    value_kind(&effect["object"]),
                    Some("chosenTargets" | "chosenObjects")
                ) && effect["object"]["id"].is_string())
                || (value_kind(&effect["object"]) == Some("eachPermanent")
                    && card_filter_supported(&effect["object"]["where"])
                    && effect["object"]
                        .get("player")
                        .is_none_or(player_reference_supported)))
                && numeric_expression_supported(&effect["power"])
                && numeric_expression_supported(&effect["toughness"])
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("untilEndOfCurrentTurn" | "untilNextTurn")
                )
                && (value_kind(&effect["duration"]) != Some("untilNextTurn")
                    || player_reference_supported(&effect["duration"]["player"]))
        }
        Some("setPower") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && numeric_expression_supported(&effect["power"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("grantFirebending") => {
            (target_reference_supported(&effect["object"])
                || value_kind(&effect["object"]) == Some("eachPermanent"))
                && numeric_expression_supported(&effect["quantity"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("setPermanentFlag") => {
            (target_reference_supported(&effect["object"])
                || matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ))
                && effect["flag"].is_string()
                && effect["value"].is_boolean()
        }
        Some("suspectPermanent") => {
            target_reference_supported(&effect["object"])
                || matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                )
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_characteristics_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_characteristics_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_characteristics_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("modifyPowerToughness") => {
                let selected_players = effect["object"]
                    .get("player")
                    .map(|player| self.resolve_players(player, stack_object, bindings));
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
                        match value_kind(&effect["object"]) {
                            Some("self" | "abilitySource") => {
                                vec![stack_object.card.instance_id.clone()]
                            }
                            Some(
                                "chosenTarget"
                                | "boundValue"
                                | "blockingPermanent"
                                | "triggeringPermanent",
                            ) => self
                                .resolve_target(&effect["object"], stack_object, bindings)
                                .and_then(|target| match target {
                                    TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                                    _ => None,
                                })
                                .unwrap_or_default(),
                            Some("chosenTargets") => self
                                .resolve_runtime_object_ids(
                                    &effect["object"],
                                    stack_object,
                                    bindings,
                                    decisions,
                                )
                                .unwrap_or_default(),
                            Some("eachPermanent") => self
                                .state
                                .players
                                .iter()
                                .flat_map(|player| player.battlefield.iter())
                                .filter(|permanent| {
                                    (!effect["object"]["excludeSource"].as_bool().unwrap_or(false)
                                        || permanent.instance_id != stack_object.card.instance_id)
                                        && (!effect["object"]["excludeSourceController"]
                                            .as_bool()
                                            .unwrap_or(false)
                                            || permanent.controller != stack_object.controller)
                                        && selected_players
                                .as_ref()
                                .is_none_or(|players| players.contains(&permanent.controller))
                                        // CR 608.2h, 611.2c: the affected set is the
                                        // permanents matching NOW, read from their
                                        // battlefield state (attacking, tapped, …)
                                        // and effective characteristics.
                                        && self.permanent_matches_effective_filter(
                                            permanent,
                                            &effect["object"]["where"],
                                        )
                                })
                                .map(|permanent| permanent.instance_id.clone())
                                .collect(),
                            _ => Vec::new(),
                        }
                    }
                };
                let power = self.checked_runtime_numeric_expression(
                    &effect["power"],
                    stack_object,
                    bindings,
                    decisions,
                    "modifyPowerToughness.power",
                )?;
                let toughness = self.checked_runtime_numeric_expression(
                    &effect["toughness"],
                    stack_object,
                    bindings,
                    decisions,
                    "modifyPowerToughness.toughness",
                )?;
                let expires_before_turn =
                    self.permanent_action_expiry_turn(&effect["duration"], stack_object, bindings)?;
                for instance_id in instance_ids {
                    if let Some(expires_before_turn) = expires_before_turn {
                        self.state.rule_modifiers.push(json!({
                            "kind": "temporaryPowerToughnessUntilTurn",
                            "cardInstanceId": instance_id,
                            "power": power,
                            "toughness": toughness,
                            "expiresBeforeTurn": expires_before_turn,
                        }));
                    } else if let Some((player_index, card_index)) =
                        self.permanent_position(&instance_id)
                    {
                        let permanent =
                            &mut self.state.players[player_index].battlefield[card_index];
                        permanent.power_modifier += power;
                        permanent.toughness_modifier += toughness;
                        self.record_event(
                            "powerToughnessModified",
                            Some(stack_object.controller.clone()),
                            Some(instance_id),
                            json!({
                                "power": power,
                                "toughness": toughness,
                                "untilTurn": self.state.turn_number,
                            }),
                        );
                    }
                }
            }
            Some("setPower") => {
                let instance_id = match value_kind(&effect["object"]) {
                    Some("self" | "abilitySource") => Some(stack_object.card.instance_id.clone()),
                    _ => self
                        .resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        }),
                };
                let power = self
                    .runtime_numeric_expression(&effect["power"], stack_object, bindings, decisions)
                    .unwrap_or(0);
                if let Some(instance_id) = instance_id
                    && self.permanent_position(&instance_id).is_some()
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "temporarySetPower",
                        "objectId": instance_id.clone(),
                        "power": power,
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                    self.record_event(
                        "basePowerChanged",
                        Some(stack_object.controller.clone()),
                        Some(instance_id),
                        json!({ "power": power, "untilTurn": self.state.turn_number }),
                    );
                }
            }
            Some("grantFirebending") => {
                let quantity = self
                    .runtime_numeric_expression(
                        &effect["quantity"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or(0)
                    .max(0);
                let instance_ids = match value_kind(&effect["object"]) {
                    Some("eachPermanent") => self
                        .state
                        .players
                        .iter()
                        .flat_map(|player| player.battlefield.iter())
                        .filter(|permanent| {
                            permanent.controller == stack_object.controller
                                && matches_card_filter(
                                    &permanent.definition,
                                    &effect["object"]["where"],
                                )
                        })
                        .map(|permanent| permanent.instance_id.clone())
                        .collect(),
                    _ => self
                        .resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                            _ => None,
                        })
                        .unwrap_or_default(),
                };
                for instance_id in instance_ids {
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    {
                        self.state.players[player_index].battlefield[card_index]
                            .definition
                            .rules
                            .push(json!({
                                "kind": "keywordAbility",
                                "source": { "kind": "self" },
                                "ability": {
                                    "kind": "firebending",
                                    "quantity": {
                                        "kind": "integer",
                                        "value": quantity,
                                    },
                                },
                                "temporaryUntilTurn": self.state.turn_number,
                            }));
                    }
                }
            }
            Some("setPermanentFlag") => {
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
                if let (Some(instance_id), Some(flag), Some(value)) = (
                    instance_id,
                    effect["flag"].as_str(),
                    effect["value"].as_bool(),
                ) && let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                {
                    self.state.players[player_index].battlefield[card_index]
                        .flags
                        .insert(flag.to_string(), value);
                }
            }
            Some("suspectPermanent") => {
                let instance_id = if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    stack_object.card.instance_id.clone()
                } else {
                    let Some(TargetRef::Permanent { instance_id }) =
                        self.resolve_target(&effect["object"], stack_object, bindings)
                    else {
                        return Ok(());
                    };
                    instance_id
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                // CR 701.60d: a suspected permanent can't become suspected again.
                if self.state.players[player_index].battlefield[card_index]
                    .flags
                    .get("suspected")
                    == Some(&true)
                {
                    return Ok(());
                }
                for keyword in ["menace", "cantBlock"] {
                    self.state.players[player_index].battlefield[card_index]
                        .definition
                        .rules
                        .push(json!({
                            "kind": "staticAbility",
                            "source": { "kind": "self" },
                            "activeWhile": {
                                "kind": "inZone",
                                "object": { "kind": "self" },
                                "zone": { "kind": "battlefield" },
                            },
                            "modifiers": [{
                                "kind": "grantKeyword",
                                "objects": { "kind": "self" },
                                "keyword": keyword,
                            }],
                        }));
                }
                self.state.players[player_index].battlefield[card_index]
                    .flags
                    .insert("suspected".to_string(), true);
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
