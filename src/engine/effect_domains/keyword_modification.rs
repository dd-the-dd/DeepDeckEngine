//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

thread_local! {
    /// Set while a granted changeling is being evaluated: a nested
    /// creature-type read inside that evaluation sees printed changeling only.
    static EVALUATING_GRANTED_CHANGELING: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// Clears the reentrancy flag even if the evaluation unwinds.
struct GrantedChangelingScope;

impl GrantedChangelingScope {
    fn enter() -> Option<Self> {
        EVALUATING_GRANTED_CHANGELING.with(|flag| {
            if flag.get() {
                None
            } else {
                flag.set(true);
                Some(Self)
            }
        })
    }
}

impl Drop for GrantedChangelingScope {
    fn drop(&mut self) {
        EVALUATING_GRANTED_CHANGELING.with(|flag| flag.set(false));
    }
}

impl GameEngine {
    /// Whether the permanent is every creature type through changeling
    /// (CR 702.73a). Printed changeling is a characteristic-defining ability
    /// applied in layer 4 before any ability loss (CR 604.3, 613.1d, 613.3).
    /// A granted changeling (a static grant, a resolved grant or a counter)
    /// is read through the effective keyword. That read evaluates static
    /// selectors and conditions which may themselves ask for creature types,
    /// so while it runs a nested changeling read sees printed changeling
    /// only: the type layer never depends on itself.
    pub(in crate::engine) fn permanent_has_changeling(&self, permanent: &CardInstance) -> bool {
        if permanent_printed_has_keyword(permanent, "changeling") {
            return true;
        }
        let Some(_scope) = GrantedChangelingScope::enter() else {
            return false;
        };
        self.permanent_has_effective_keyword(permanent, "changeling")
    }
}

/// A keyword a resolved effect removed for as long as the object stays on
/// the battlefield (`it loses defender`). The marker lives on the object, so
/// a zone change ends it (CR 400.7); it is not a copiable value (CR 707.2).
pub(in crate::engine) const KEYWORD_LOSS_MARKER: &str = "keywordLossModification";

/// Whether the latest loss marker of `keyword` on this object is still in
/// force: a later resolved grant of the same keyword, recorded after it,
/// has a later timestamp and restores the keyword (CR 613.7).
fn marker_removes_keyword(definition: &CardDefinition, keyword: &str) -> bool {
    let rules = definition.printed_rules();
    let Some(loss) = rules.iter().rposition(|rule| {
        value_kind(rule) == Some(KEYWORD_LOSS_MARKER) && rule["keyword"].as_str() == Some(keyword)
    }) else {
        return false;
    };
    !rules[loss + 1..].iter().any(|rule| {
        value_kind(rule) == Some("keywordAbility") && value_kind(&rule["ability"]) == Some(keyword)
    })
}

impl GameEngine {
    /// Records an indefinite keyword loss on a battlefield object.
    fn record_keyword_loss_marker(&mut self, instance_id: &str, keyword: &str) {
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return;
        };
        self.state.players[player_index].battlefield[card_index]
            .definition
            .printed_rules_mut()
            .push(json!({ "kind": KEYWORD_LOSS_MARKER, "keyword": keyword }));
    }

    /// Whether a resolved `loses KEYWORD` effect removes `keyword` from the
    /// permanent now: an until-end-of-turn loss of this turn, or an
    /// indefinite loss marker recorded on the object.
    pub(in crate::engine) fn keyword_removed_by_resolved_effect(
        &self,
        permanent: &CardInstance,
        keyword: &str,
    ) -> bool {
        self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("temporaryKeywordLoss")
                && modifier["cardInstanceId"].as_str() == Some(permanent.instance_id.as_str())
                && modifier["keyword"].as_str() == Some(keyword)
                && modifier["expiresAfterTurn"].as_u64() == Some(u64::from(self.state.turn_number))
        }) || marker_removes_keyword(&permanent.definition, keyword)
    }
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        // CR 611.2c: a resolved `loses KEYWORD` affects the permanents its
        // operand names when it begins: a static selector, or any battlefield
        // object operand (a target, the source, a bound or observed object, a
        // battlefield collection). A card-zone operand stays unsupported.
        Some("removeKeywordsFromPermanents") => {
            (static_object_selector_supported(&effect["objects"])
                || super::battlefield_object_operand_supported(&effect["objects"]))
                && effect["keywords"].as_array().is_some_and(|keywords| {
                    !keywords.is_empty()
                        && keywords.iter().all(|keyword| {
                            keyword.as_str().is_some_and(|keyword| {
                                keyword_kind_supported(&json!({ "kind": keyword }))
                            })
                        })
                })
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("untilEndOfCurrentTurn" | "permanent")
                )
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
                    || keyword_rule_supported(&effect["ability"])
                    || value_kind(&effect["ability"]) == Some("staticAbility")
                        && rule_supported(&effect["ability"]));
            let flashback_supported = target_reference_supported(&effect["object"])
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && value_kind(&effect["ability"]) == Some("flashback")
                && value_kind(&effect["ability"]["cost"]) == Some("manaCostOf")
                && value_kind(&effect["ability"]["cost"]["card"]) == Some("abilitySource");
            // The source or one target gains a triggered ability until end
            // of turn (CR 611.2a); execution appends it to that object.
            let temporary_trigger_supported = (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
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
                    || mana_rule_supported(&effect["ability"])
                    || keyword_rule_supported(&effect["ability"]));
            let temporary_single_supported = (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                && (activated_rule_supported(&effect["ability"])
                    || mana_rule_supported(&effect["ability"])
                    // A granted parameterized keyword (CR 702.1): Ward,
                    // protection or a count keyword read from the rules.
                    || keyword_rule_supported(&effect["ability"]));
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
        // The affected set is any battlefield object operand, locked in at
        // resolution (CR 611.2c).
        Some("loseAllAbilitiesPermanently") => {
            super::battlefield_object_operand_supported(&effect["object"])
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
                // CR 611.2c: the affected set is locked in at resolution.
                let targets = if matches!(
                    value_kind(&effect["objects"]),
                    Some("permanents" | "attachedPermanent" | "spells")
                ) {
                    self.state
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
                        .collect::<Vec<_>>()
                } else {
                    self.battlefield_operand_ids(
                        &effect["objects"],
                        stack_object,
                        bindings,
                        decisions,
                    )?
                };
                let lasts_indefinitely = value_kind(&effect["duration"]) == Some("permanent");
                for instance_id in targets {
                    for keyword in &keywords {
                        if lasts_indefinitely {
                            self.record_keyword_loss_marker(&instance_id, keyword);
                        } else {
                            self.state.rule_modifiers.push(json!({
                                "kind": "temporaryKeywordLoss",
                                "cardInstanceId": instance_id,
                                "keyword": keyword,
                                "expiresAfterTurn": self.state.turn_number,
                            }));
                        }
                    }
                    self.record_event(
                        "keywordsRemoved",
                        Some(stack_object.controller.clone()),
                        Some(instance_id),
                        json!({ "keywords": keywords, "duration": effect["duration"] }),
                    );
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
                // CR 611.2c: every object of the operand, locked in now.
                for instance_id in self.battlefield_operand_ids(
                    &effect["object"],
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    else {
                        continue;
                    };
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
                                                self.permanent_has_changeling(permanent)
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

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "keyword_modification_tests.rs"]
mod tests;
