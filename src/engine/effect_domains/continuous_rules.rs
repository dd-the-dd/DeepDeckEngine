//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("installUntapRestriction") => {
            target_reference_supported(&effect["permanent"])
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("nextUntapStep" | "whileSourceControlled")
                )
        }
        Some("installRegenerationShield") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        // CR 701.15a over a collection: `objects` is a shared object expression.
        Some("installRegenerationShields") => {
            object_expression_supported(&effect["objects"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("preventCombatDamageFromPermanent") => {
            target_reference_supported(&effect["permanent"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("increaseOpponentSpellCostUntilNextTurn") => {
            player_reference_supported(&effect["player"])
                && card_filter_supported(&effect["where"])
                && integer_value(&effect["amount"]).is_some_and(|amount| amount > 0)
        }
        Some("installDeathExileReplacement") => {
            target_reference_supported(&effect["object"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("installRuleModifier") => {
            (value_kind(&effect["modifier"]) == Some("damageCantBePrevented")
                || (value_kind(&effect["modifier"]) == Some("reduceCastingCost")
                    && battlefield_static_modifier_supported(&effect["modifier"])))
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("untilEndOfCurrentTurn" | "untilNextTurn" | "permanent")
                )
                && (value_kind(&effect["duration"]) != Some("untilNextTurn")
                    || player_reference_supported(&effect["duration"]["player"]))
        }
        Some("installCreatureDeathLifeGainUntilEndOfTurn") => {
            player_reference_supported(&effect["player"])
                && integer_value(&effect["amount"]).is_some_and(|amount| amount > 0)
        }
        Some("installDamageMultiplier") => {
            player_reference_supported(&effect["player"])
                && integer_value(&effect["factor"]).is_some_and(|factor| factor > 1)
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        // CR 615: a prevention shield over a damage amount (`all` or a numeric
        // expression), optionally restricted to combat damage, one recipient
        // and one source, lasting for the current turn.
        Some("preventDamage") => {
            (value_kind(&effect["amount"]) == Some("all")
                || numeric_expression_supported(&effect["amount"]))
                && effect.get("combatOnly").is_none_or(Value::is_boolean)
                && effect
                    .get("recipient")
                    .is_none_or(prevention_party_supported)
                && effect.get("source").is_none_or(prevention_party_supported)
                // `a SOURCE of your choice` (CR 609.7a) and the `next time`
                // shield that applies once (CR 615.7).
                && effect.get("sourceChoice").is_none_or(|choice| {
                    effect.get("source").is_none() && card_filter_supported(&choice["where"])
                })
                && effect.get("nextTimeOnly").is_none_or(Value::is_boolean)
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("installCounterLinkedCharacteristics") => {
            target_reference_supported(&effect["object"])
                && effect["counter"].is_string()
                && effect["addSubtypes"].as_array().is_some_and(|subtypes| {
                    !subtypes.is_empty() && subtypes.iter().all(Value::is_string)
                })
                && effect["keywords"].as_array().is_some_and(|keywords| {
                    !keywords.is_empty()
                        && keywords.iter().all(|keyword| {
                            keyword.as_str().is_some_and(|kind| {
                                keyword_kind_supported(&json!({ "kind": kind }))
                            })
                        })
                })
        }
        Some("installAdditionalManaOnLandTap") => {
            card_filter_supported(&effect["where"])
                && effect.get("player").is_none_or(player_reference_supported)
                && matches!(
                    effect["mana"].as_str(),
                    Some("W" | "U" | "B" | "R" | "G" | "C")
                )
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("installLinkedKeyword") => {
            target_reference_supported(&effect["object"])
                && effect["keyword"]
                    .as_str()
                    .is_some_and(|keyword| keyword_kind_supported(&json!({ "kind": keyword })))
                && (effect["whileSourceControlled"].as_bool() == Some(true)
                    || value_kind(&effect["duration"]) == Some("whileSourceOnBattlefield"))
        }
        Some("installLinkedAbilityLoss") => {
            target_reference_supported(&effect["object"])
                && value_kind(&effect["duration"]) == Some("whileSourceOnBattlefield")
        }
        Some("installLinkedDamagePrevention") => {
            target_reference_supported(&effect["source"])
                && value_kind(&effect["duration"]) == Some("whileSourceOnBattlefield")
        }
        Some("installOneShotModifier") => {
            super::super::one_shot_modifier::install_one_shot_modifier_supported(effect)
        }
        _ => false,
    }
}

/// A damage source or recipient: the ability's own source, a target/bound
/// object reference or a player reference.
fn prevention_party_supported(reference: &Value) -> bool {
    matches!(value_kind(reference), Some("self" | "abilitySource"))
        || target_reference_supported(reference)
        || player_reference_supported(reference)
}

impl GameEngine {
    fn resolve_prevention_party(
        &self,
        reference: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) -> Option<TargetRef> {
        if matches!(value_kind(reference), Some("self" | "abilitySource")) {
            return Some(TargetRef::Permanent {
                instance_id: stack_object.card.instance_id.clone(),
            });
        }
        if target_reference_supported(reference) {
            return self.resolve_target(reference, stack_object, bindings);
        }
        self.resolve_player(reference, stack_object, bindings)
            .map(|player_id| TargetRef::Player { player_id })
    }

    pub(super) fn execute_continuous_rules_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        // A source chosen on resolution becomes the shield's source.
        if let Some(chosen) =
            self.choose_prevention_source(effect, stack_object, decisions, provider)?
        {
            return self
                .execute_continuous_rules_domain_known(&chosen, stack_object, bindings, decisions)
                .map(|()| true);
        }
        self.execute_continuous_rules_domain_known(effect, stack_object, bindings, decisions)
            .map(|()| true)
    }

    fn execute_continuous_rules_domain_known(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("installUntapRestriction") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                {
                    let kind = if value_kind(&effect["duration"]) == Some("nextUntapStep") {
                        "skipNextUntap"
                    } else {
                        "doesNotUntapWhileSourceControlled"
                    };
                    self.state.rule_modifiers.push(json!({
                        "kind": kind,
                        "sourceCardInstanceId": stack_object.card.instance_id,
                        "cardInstanceId": instance_id,
                    }));
                }
            }
            Some("installRegenerationShield") => {
                let instance_id = match value_kind(&effect["object"]) {
                    Some("self" | "abilitySource") => stack_object.card.instance_id.clone(),
                    _ => self
                        .resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        })
                        .unwrap_or_default(),
                };
                self.install_regeneration_shield(&instance_id);
            }
            Some("installRegenerationShields") => {
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
                    if attempted.insert(instance_id.clone()) {
                        self.install_regeneration_shield(&instance_id);
                    }
                }
            }
            Some("installDeathExileReplacement") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                    && self.permanent_position(&instance_id).is_some()
                {
                    // CR 614.1a: a one-shot replacement that lasts this turn
                    // only; the departure pipeline checks its turn.
                    self.state.rule_modifiers.push(json!({
                        "kind": "exileIfDiesThisTurn",
                        "instanceId": instance_id,
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("installRuleModifier") => {
                let modifier = if value_kind(&effect["modifier"]) == Some("reduceCastingCost") {
                    let Some(player_id) =
                        self.resolve_player(&effect["modifier"]["player"], stack_object, bindings)
                    else {
                        return Ok(());
                    };
                    let amount = self
                        .runtime_numeric_expression(
                            &effect["modifier"]["amount"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .unwrap_or(0)
                        .max(0);
                    json!({
                        "kind": "temporaryCastingCostReduction",
                        "playerId": player_id,
                        "where": effect["modifier"]["where"],
                        "amount": amount,
                        "expiresAfterTurn": self.state.turn_number,
                        "sourceCardInstanceId": stack_object.card.instance_id,
                    })
                } else {
                    json!({
                        "duration": effect["duration"],
                        "expiresAfterTurn": self.state.turn_number,
                        "modifier": effect["modifier"],
                        "sourceCardInstanceId": stack_object.card.instance_id,
                    })
                };
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "ruleModifierInstalled",
                    Some(stack_object.controller.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("installCreatureDeathLifeGainUntilEndOfTurn") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "creatureDeathLifeGainUntilEndOfTurn",
                        "playerId": player_id,
                        "amount": integer_value(&effect["amount"]).unwrap_or(1),
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("installDamageMultiplier") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let modifier = json!({
                    "kind": "damageMultiplier",
                    "playerId": player_id,
                    "factor": effect["factor"],
                    "expiresAfterTurn": self.state.turn_number,
                });
                self.state.rule_modifiers.push(modifier.clone());
                self.record_event(
                    "damageMultiplierInstalled",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    modifier,
                );
            }
            Some("preventCombatDamageFromPermanent") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["permanent"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "preventCombatDamageByPermanent",
                        "permanentId": instance_id,
                        "expiresAfterTurn": self.state.turn_number,
                    }));
                }
            }
            Some("preventDamage") => {
                // `all` prevents every matching damage event; otherwise the
                // shield holds the resolved amount (CR 615.7).
                let remaining = if value_kind(&effect["amount"]) == Some("all") {
                    None
                } else {
                    let amount = self
                        .runtime_numeric_expression(
                            &effect["amount"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .unwrap_or(0);
                    if amount <= 0 {
                        return Ok(());
                    }
                    Some(amount)
                };
                // A party whose reference no longer resolves (an illegal
                // target) leaves the effect without a shield.
                let recipient = match effect.get("recipient") {
                    None => Value::Null,
                    Some(reference) => {
                        match self.resolve_prevention_party(reference, stack_object, bindings) {
                            Some(TargetRef::Player { player_id }) => {
                                json!({ "playerId": player_id })
                            }
                            Some(TargetRef::Permanent { instance_id }) => {
                                json!({ "permanentId": instance_id })
                            }
                            _ => return Ok(()),
                        }
                    }
                };
                if effect["noSource"].as_bool() == Some(true) {
                    return Ok(());
                }
                let source = match effect.get("source") {
                    None if effect["chosenSourceId"].is_string() => {
                        effect["chosenSourceId"].clone()
                    }
                    None => Value::Null,
                    Some(reference) => {
                        match self.resolve_prevention_party(reference, stack_object, bindings) {
                            Some(TargetRef::Permanent { instance_id }) => json!(instance_id),
                            _ => return Ok(()),
                        }
                    }
                };
                let shield_id = self
                    .state
                    .rule_modifiers
                    .iter()
                    .filter(|modifier| value_kind(modifier) == Some("damagePreventionShield"))
                    .filter_map(|modifier| modifier["shieldId"].as_u64())
                    .max()
                    .map_or(1, |id| id + 1);
                self.state.rule_modifiers.push(json!({
                    "kind": "damagePreventionShield",
                    "shieldId": shield_id,
                    "remaining": remaining,
                    "combatOnly": effect["combatOnly"].as_bool().unwrap_or(false),
                    "recipient": recipient,
                    "sourcePermanentId": source,
                    "nextTimeOnly": effect["nextTimeOnly"].as_bool().unwrap_or(false),
                    "expiresAfterTurn": self.state.turn_number,
                }));
            }
            Some("installCounterLinkedCharacteristics") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "counterLinkedCharacteristics",
                    "cardInstanceId": instance_id,
                    "counter": effect["counter"],
                    "addSubtypes": effect["addSubtypes"],
                    "keywords": effect["keywords"],
                }));
            }
            Some("increaseOpponentSpellCostUntilNextTurn") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let expires_before_turn = self
                    .next_turn_number_for_player(&player_id)
                    .unwrap_or_else(|| self.state.turn_number.saturating_add(1));
                self.state.rule_modifiers.push(json!({
                    "kind": "temporaryOpponentSpellCostIncrease",
                    "playerId": player_id,
                    "where": effect["where"].clone(),
                    "amount": effect["amount"].clone(),
                    "expiresBeforeTurn": expires_before_turn,
                }));
            }
            Some("installAdditionalManaOnLandTap") => {
                let mut modifier = json!({
                    "kind": "additionalManaOnLandTap",
                    "where": effect["where"],
                    "mana": effect["mana"],
                    "expiresAfterTurn": self.state.turn_number,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                });
                if let Some(player_id) = effect
                    .get("player")
                    .and_then(|player| self.resolve_player(player, stack_object, bindings))
                {
                    modifier["playerId"] = Value::String(player_id);
                }
                self.state.rule_modifiers.push(modifier);
            }
            Some("installLinkedKeyword") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "linkedKeyword",
                        "sourceCardInstanceId": stack_object.card.instance_id,
                        "sourceController": stack_object.controller,
                        "requiresSameController": effect["whileSourceControlled"]
                            .as_bool()
                            .unwrap_or(false),
                        "targetCardInstanceId": instance_id,
                        "keyword": effect["keyword"],
                    }));
                }
            }
            Some("installLinkedAbilityLoss") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "linkedAbilityLoss",
                        "sourceCardInstanceId": stack_object.card.instance_id,
                        "targetCardInstanceId": instance_id,
                    }));
                }
            }
            Some("installLinkedDamagePrevention") => {
                if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["source"], stack_object, bindings)
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "linkedDamagePrevention",
                        "sourceCardInstanceId": stack_object.card.instance_id,
                        "damagingCardInstanceId": instance_id,
                    }));
                }
            }
            Some("installOneShotModifier") => {
                self.install_one_shot_modifier(effect, stack_object, bindings);
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
