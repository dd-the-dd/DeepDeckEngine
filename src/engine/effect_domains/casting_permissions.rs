//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("prohibitAbilityClassDuringGrantedExtraTurn") => {
            player_reference_supported(&effect["player"]) && effect["abilityClass"].is_string()
        }
        Some("grantAdditionalLandPlays") => {
            player_reference_supported(&effect["player"])
                && integer_value(&effect["count"]).is_some_and(|count| count > 0)
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("grantZoneCastingUntilEndOfTurn") => {
            player_reference_supported(&effect["player"])
                && matches!(effect["sourceZone"].as_str(), Some("graveyard" | "exile"))
                && card_filter_supported(&effect["where"])
        }
        Some("installNextCreatureSpellModifier") => {
            player_reference_supported(&effect["player"])
                && value_kind(&effect["expiresAfterTurn"]) == Some("currentTurn")
                && effect["grantFlash"].is_boolean()
                && effect["cantBeCountered"].is_boolean()
                && effect["enteringCounter"].is_string()
                && numeric_expression_supported(&effect["enteringCounterCount"])
        }
        Some("installTemporaryCantBeCountered") => {
            player_reference_supported(&effect["player"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }
        Some("grantCardPermission") => {
            let alternative_cost_supported =
                effect["play"].get("alternativeCost").is_none_or(|cost| {
                    value_kind(cost) == Some("payLife")
                        && player_reference_supported(&cost["player"])
                        && value_kind(&cost["amount"]) == Some("manaValueOf")
                        && value_kind(&cost["amount"]["object"]) == Some("grantedCard")
                });
            player_reference_supported(&effect["player"])
                && object_expression_supported(&effect["cards"])
                && effect["permissions"].as_array().is_some_and(|permissions| {
                    permissions
                        .iter()
                        .all(|permission| matches!(permission.as_str(), Some("lookAt" | "play")))
                })
                && effect["play"]["normalTimingApplies"].as_bool() == Some(true)
                && effect["play"]["normalCostsApply"].is_boolean()
                && matches!(
                    value_kind(&effect["play"]["castingModifier"]),
                    Some("none" | "spendManaAsAnyType")
                )
                && alternative_cost_supported
                && (value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                    || value_kind(&effect["duration"]) == Some("whileObjectsRemainInZone")
                        && value_kind(&effect["duration"]["zone"]) == Some("exile"))
                && effect.get("condition").is_none_or(condition_supported)
        }
        Some("grantHandPlayPermission") => {
            player_reference_supported(&effect["player"])
                && player_reference_supported(&effect["handOwner"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
        }

        Some("grantPermission") => {
            player_reference_supported(&effect["player"])
                && matches!(value_kind(&effect["action"]), Some("play" | "cast"))
                && permission_card_reference_supported(&effect["action"]["card"])
                && effect["action"]["normalTimingApplies"].as_bool() == Some(true)
                && effect["action"]["normalCostsApply"].is_boolean()
                && effect
                    .get("replacePermissionsFromSource")
                    .is_none_or(Value::is_boolean)
                && effect["action"]
                    .get("spendManaAsAnyType")
                    .is_none_or(Value::is_boolean)
                && matches!(
                    value_kind(&effect["duration"]),
                    Some(
                        "untilEndOfCurrentTurn"
                            | "untilEndOfNextTurn"
                            | "untilNextEndStep"
                            | "whileSourceControlled"
                            | "whileInExile"
                    )
                )
        }
        Some("replaceCastSpellDestination") => {
            object_expression_supported(&effect["spells"])
                && card_filter_supported(&effect["where"])
                && effect["destination"].as_str() == Some("exile")
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_casting_permissions_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        _provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_casting_permissions_domain_known(effect, stack_object, bindings, decisions)
            .map(|()| true)
    }

    fn execute_casting_permissions_domain_known(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("installNextCreatureSpellModifier") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "nextCreatureSpellModifier",
                    "playerId": player_id,
                    "grantFlash": effect["grantFlash"],
                    "cantBeCountered": effect["cantBeCountered"],
                    "enteringCounter": effect["enteringCounter"],
                    "enteringCounterCount": effect["enteringCounterCount"],
                    "expiresAfterTurn": self.state.turn_number,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                }));
            }
            Some("installTemporaryCantBeCountered") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "temporaryCantBeCountered",
                    "playerId": player_id,
                    "expiresAfterTurn": self.state.turn_number,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                }));
            }
            Some("grantCardPermission") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let cards = self
                    .resolve_object_ids(&effect["cards"], bindings, decisions)
                    .unwrap_or_default();
                for card_id in cards {
                    // CR 716.3a: the life paid is the mana value of what is
                    // actually cast (an Adventure or the creature), so it is
                    // evaluated at cast time, not when the permission is granted.
                    let alternative_life_cost = effect["play"]
                        .get("alternativeCost")
                        .filter(|cost| value_kind(cost) == Some("payLife"))
                        .is_some_and(|cost| {
                            value_kind(&cost["amount"]) == Some("manaValueOf")
                                && value_kind(&cost["amount"]["object"]) == Some("grantedCard")
                        })
                        .then(
                            || json!({ "kind": "manaValueOf", "object": { "kind": "castCard" } }),
                        );
                    let mut permission = json!({
                        "kind": "cardPermission",
                        "action": "play",
                        "cardInstanceId": card_id.clone(),
                        "canLookAt": true,
                        "normalCostsApply": effect["play"]["normalCostsApply"],
                        "normalTimingApplies": true,
                        "playerId": player_id.clone(),
                        "sourceCardInstanceId": stack_object.card.instance_id,
                        "spendManaAsAnyType": value_kind(&effect["play"]["castingModifier"])
                            == Some("spendManaAsAnyType"),
                        "whileInZone": "exile",
                    });
                    if value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn") {
                        permission["expiresAfterTurn"] = Value::from(self.state.turn_number);
                    }
                    if let Some(amount) = alternative_life_cost {
                        permission["alternativeLifeCost"] = amount;
                    }
                    if let Some(condition) = effect.get("condition") {
                        permission["condition"] = condition.clone();
                    }
                    self.bind_card_permission_to_incarnation(&mut permission);
                    self.state.permissions.push(permission.clone());
                    self.record_event(
                        "cardPermissionGranted",
                        Some(player_id.clone()),
                        Some(card_id),
                        permission,
                    );
                }
            }
            Some("prohibitAbilityClassDuringGrantedExtraTurn") => {
                if let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                    && let Some(ability_class) = effect["abilityClass"].as_str()
                {
                    self.state.rule_modifiers.push(json!({
                        "kind": "prohibitAbilityClassActivationDuringTurn",
                        "playerId": player_id,
                        "abilityClass": ability_class,
                        "turnNumber": self.state.turn_number.saturating_add(1),
                    }));
                }
            }
            Some("grantAdditionalLandPlays") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let count = integer_value(&effect["count"]).unwrap_or(1).max(0);
                self.state.rule_modifiers.push(json!({
                    "kind": "additionalLandPlay",
                    "playerId": player_id,
                    "count": count,
                    "expiresAfterTurn": self.state.turn_number,
                }));
            }
            Some("grantZoneCastingUntilEndOfTurn") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                self.state.rule_modifiers.push(json!({
                    "kind": "temporaryCastingPermission",
                    "playerId": player_id,
                    "sourceCardInstanceId": stack_object.card.instance_id,
                    "sourceZone": effect["sourceZone"],
                    "where": effect["where"],
                    "expiresAfterTurn": self.state.turn_number,
                }));
            }
            Some("grantHandPlayPermission") => {
                let permitted_player =
                    self.resolve_player(&effect["player"], stack_object, bindings);
                let hand_owner = self.resolve_player(&effect["handOwner"], stack_object, bindings);
                if let (Some(permitted_player), Some(hand_owner)) = (permitted_player, hand_owner) {
                    let hand_cards = self
                        .player_index(&hand_owner)
                        .ok()
                        .map(|player_index| self.state.players[player_index].hand.clone())
                        .unwrap_or_default();
                    for card in hand_cards {
                        self.state.permissions.push(json!({
                            "kind": "playCardPermission",
                            "playerId": permitted_player,
                            "cardInstanceId": card.instance_id,
                            "sourceZone": "hand",
                            "sourceOwnerId": hand_owner,
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                            "expiresAfterTurn": self.state.turn_number,
                        }));
                    }
                }
            }

            Some("replaceCastSpellDestination") => {
                let card_ids = self
                    .resolve_runtime_object_ids(
                        &effect["spells"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .unwrap_or_default()
                    .into_iter()
                    .collect::<BTreeSet<_>>();
                for spell in &mut self.state.stack {
                    if card_ids.contains(&spell.card.instance_id)
                        && matches_card_filter(&spell.card.definition, &effect["where"])
                    {
                        spell.exile_on_leave_stack = true;
                    }
                }
            }
            Some("grantPermission") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let card_reference = &effect["action"]["card"];
                let objects = if let Some(binding_id) = card_reference["binding"].as_str() {
                    bindings
                        .get(binding_id)
                        .and_then(|binding| match binding {
                            RuntimeBinding::Objects(objects) => Some(objects.clone()),
                            _ => None,
                        })
                        .unwrap_or_default()
                } else if value_kind(card_reference) == Some("decisionResult") {
                    card_reference["decisionId"]
                        .as_str()
                        .and_then(|decision_id| decisions.get(decision_id))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                } else {
                    self.resolve_target(card_reference, stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Card { instance_id }
                            | TargetRef::Permanent { instance_id } => Some(vec![instance_id]),
                            TargetRef::Player { .. } | TargetRef::StackObject { .. } => None,
                        })
                        .unwrap_or_default()
                };
                let expires_after_turn = match value_kind(&effect["duration"]) {
                    Some("untilEndOfNextTurn" | "untilNextEndStep") => self
                        .next_turn_number_for_player(&player_id)
                        .unwrap_or(self.state.turn_number),
                    _ => self.state.turn_number,
                };
                if effect["replacePermissionsFromSource"].as_bool() == Some(true) {
                    self.state.permissions.retain(|permission| {
                        permission["sourceCardInstanceId"].as_str()
                            != Some(stack_object.card.instance_id.as_str())
                    });
                }
                for card_id in objects {
                    if value_kind(&effect["action"]) == Some("cast")
                        && self
                            .card_outside_battlefield(&card_id)
                            .is_some_and(|card| is_land(&card.definition))
                    {
                        continue;
                    }
                    let mut permission = json!({
                        "action": value_kind(&effect["action"]).unwrap_or("play"),
                        "cardInstanceId": card_id,
                        "normalCostsApply": effect["action"]["normalCostsApply"],
                        "normalTimingApplies": effect["action"]["normalTimingApplies"],
                        "playerId": player_id,
                        "sourceCardInstanceId": stack_object.card.instance_id,
                    });
                    if value_kind(&effect["duration"]) == Some("whileSourceControlled") {
                        permission["whileSourceControlled"] = Value::Bool(true);
                    } else if value_kind(&effect["duration"]) == Some("whileInExile") {
                        permission["whileInZone"] = Value::String("exile".to_string());
                        permission["sourceZone"] = Value::String("exile".to_string());
                    } else {
                        permission["expiresAfterTurn"] = Value::from(expires_after_turn);
                    }
                    if value_kind(&effect["duration"]) == Some("untilNextEndStep") {
                        permission["expiresAtEndStepOf"] = Value::String(player_id.clone());
                    }
                    // CR 609.4b: mana of any type can be spent to cast it.
                    if effect["action"]["spendManaAsAnyType"].as_bool() == Some(true) {
                        permission["spendManaAsAnyType"] = Value::Bool(true);
                    }
                    self.bind_card_permission_to_incarnation(&mut permission);
                    self.state.permissions.push(permission.clone());
                    self.record_event(
                        "permissionGranted",
                        Some(player_id.clone()),
                        Some(card_id),
                        permission,
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
