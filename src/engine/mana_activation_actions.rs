//! Shared battlefield mana declaration generation. All three fallible
//! boundaries are explicit; historical callers supply Infallible callbacks.
use super::mana_checked_discovery::CheckedManaActivationPlan;
use super::mana_projection_context::ManaProjectionContext;
use super::*;

impl GameEngine {
    pub(super) fn legacy_mana_activation_actions(&self, player_index: usize) -> Vec<LegalAction> {
        let mut choices = |source: &CardInstance| {
            Ok::<_, std::convert::Infallible>(
                self.mana_choices_for_permanent(source, None, true, true),
            )
        };
        let mut declare = |rule: &Value, controller: &str, source: &str| {
            Ok::<_, std::convert::Infallible>(
                self.declarations_for_activated_rule(rule, controller, source),
            )
        };
        let mut payment = |player_index: usize,
                           costs: &[String],
                           excluded: &BTreeSet<String>,
                           spending: Option<&CardDefinition>,
                           ability_source: Option<&CardDefinition>| {
            Ok::<_, std::convert::Infallible>(self.payment_for_mana_costs_with_context(
                player_index,
                costs,
                excluded,
                spending,
                ability_source,
                None,
            ))
        };
        match self.try_mana_activation_actions_with(
            player_index,
            &mut choices,
            &mut declare,
            &mut payment,
        ) {
            Ok(actions) => actions,
            Err(never) => match never {},
        }
    }

    /// This checked route requires a fallible declaration reader. It never
    /// silently reuses the legacy X-budget query. The same readonly frame is
    /// available to declarations, production and payment for this generation.
    pub(super) fn try_checked_mana_activation_actions<D>(
        &self,
        player_index: usize,
        declare: &mut D,
    ) -> Result<Vec<LegalAction>, EngineError>
    where
        D: FnMut(
            &Value,
            &str,
            &str,
            &ManaProjectionContext<'_>,
        ) -> Result<Option<Vec<CastingDeclaration>>, EngineError>,
    {
        self.state.players.get(player_index).ok_or_else(|| EngineError::new(format!(
            "unsupported checked mana discovery: state=mana.discovery.player-index invalid index {player_index}")))?;
        let context = ManaProjectionContext::for_value_interpretation(self);
        let declared = self.try_mana_activation_actions_with(
            player_index,
            &mut |source| {
                self.checked_mana_activation_plan_in_context(source, None, true, true, &context)
                    .map(CheckedManaActivationPlan::into_activation_choices)
            },
            &mut |rule, controller, source| declare(rule, controller, source, &context),
            &mut |index, costs, excluded, spending, ability_source| {
                self.checked_payment_in_context(
                    index,
                    costs,
                    excluded,
                    spending,
                    ability_source,
                    None,
                    &context,
                )
            },
        )?;
        self.refine_postcost_mana_actions(declared, &context)
    }

    pub(super) fn refine_postcost_mana_actions(
        &self,
        declared: Vec<LegalAction>,
        context: &ManaProjectionContext<'_>,
    ) -> Result<Vec<LegalAction>, EngineError> {
        let mut actions = Vec::new();
        let mut refined_declarations = BTreeSet::new();
        for action in declared {
            let id = action
                .card_instance_id
                .as_deref()
                .ok_or_else(|| EngineError::new("declared mana action has no source"))?;
            let (p, c) = self
                .permanent_position(id)
                .ok_or_else(|| EngineError::new("declared mana source is absent"))?;
            let source = &self.state.players[p].battlefield[c];
            let origin = granted_mana_program::declared_origin(&action)?;
            let rule = self.resolve_mana_program(source, origin.as_ref())?;
            let needs_preview = self.mana_production_requires_postcost(rule.as_ref());
            if !needs_preview {
                actions.push(action);
                continue;
            }
            let symbols = action.decisions["manaProduced"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let choice = self
                .checked_mana_activation_plan_in_context(source, None, true, true, context)?
                .into_activation_choices()
                .into_iter()
                .find(|choice| {
                    choice.origin == origin
                        && choice.symbols == symbols
                        && choice.spend_restriction.as_ref()
                            == action.decisions.get("manaSpendRestriction")
                })
                .ok_or_else(|| EngineError::new("declared mana output has no producer"))?;
            let postcost = match self.postcost_choices_for_declared_action(&action, &choice)? {
                mana_postcost_preview::PostcostManaPreview::Complete(choices) => choices,
                mana_postcost_preview::PostcostManaPreview::DeferredNeedsProvider {
                    request_id,
                } => {
                    let mut unresolved = action;
                    unresolved
                        .decisions
                        .insert("manaDeclaredOutput".to_string(), json!(symbols));
                    unresolved.decisions.insert("manaProductionPreview".to_string(), json!({
                        "kind":"deferredNeedsProvider", "certified":false, "requestId":request_id,
                    }));
                    // manaProduced is retained as the historical declaration
                    // selection hint only. The typed marker denies any claim
                    // that its quantity/type is an executable post-cost fact.
                    actions.push(unresolved);
                    continue;
                }
            };
            for (index, result) in postcost.into_iter().enumerate() {
                let mut refined = action.clone();
                refined.id = format!("{}:postcost:{index}", action.id);
                refined
                    .decisions
                    .insert("manaDeclaredOutput".to_string(), json!(symbols));
                refined
                    .decisions
                    .insert("manaProduced".to_string(), json!(result.symbols));
                if result.symbols != symbols {
                    let old = symbols
                        .iter()
                        .map(|s| format!("{{{s}}}"))
                        .collect::<String>();
                    let actual = result
                        .symbols
                        .iter()
                        .map(|s| format!("{{{s}}}"))
                        .collect::<String>();
                    if !old.is_empty() {
                        refined.label = refined.label.replace(&old, &actual);
                    }
                }
                // Different pre-cost output witnesses can lead to the same
                // fully declared post-cost choice. They are not distinct
                // player decisions. Keep the whole semantic declaration key.
                let mut semantic = refined.clone();
                semantic.id.clear();
                semantic.label.clear();
                semantic.decisions.remove("manaDeclaredOutput");
                let key = serde_json::to_string(&semantic).map_err(|error| {
                    EngineError::new(format!("mana declaration serialization failed: {error}"))
                })?;
                if refined_declarations.insert(key) {
                    actions.push(refined);
                }
            }
        }
        Ok(actions)
    }

    pub(super) fn try_mana_activation_actions_with<C, D, P, E>(
        &self,
        player_index: usize,
        resolve_choices: &mut C,
        declare: &mut D,
        resolve_payment: &mut P,
    ) -> Result<Vec<LegalAction>, E>
    where
        C: FnMut(&CardInstance) -> Result<Vec<ManaChoice>, E>,
        D: FnMut(&Value, &str, &str) -> Result<Option<Vec<CastingDeclaration>>, E>,
        P: FnMut(
            usize,
            &[String],
            &BTreeSet<String>,
            Option<&CardDefinition>,
            Option<&CardDefinition>,
        ) -> Result<Option<ManaPayment>, E>,
    {
        let player = &self.state.players[player_index];
        let mut actions = Vec::new();
        for permanent in &player.battlefield {
            if self.player_activations_prohibited(&player.id) {
                continue;
            }
            if self.activation_prohibited(permanent, true) {
                continue;
            }
            for (choice_index, choice) in resolve_choices(permanent)?.into_iter().enumerate() {
                let rule = choice.program(permanent);
                let has_tap_cost = rule.map_or(true, |rule| {
                    rule["costs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|cost| value_kind(cost) == Some("tap"))
                });
                if has_tap_cost && self.permanent_cannot_pay_tap_symbol(permanent) {
                    continue;
                }
                if player.life < choice.life_cost {
                    continue;
                }
                let declarations = if let Some(rule) = rule {
                    let Some(declarations) = declare(rule, &player.id, &permanent.instance_id)?
                    else {
                        continue;
                    };
                    declarations
                } else {
                    vec![CastingDeclaration {
                        decisions: BTreeMap::new(),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                    }]
                };
                let mana_costs = rule
                    .and_then(|rule| rule["costs"].as_array())
                    .into_iter()
                    .flatten()
                    .filter(|cost| value_kind(cost) == Some("payMana"))
                    .filter_map(|cost| cost["manaCost"].as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>();
                let sacrifice_target_ids = rule
                    .map(Self::activated_cost_target_ids)
                    .unwrap_or_default();
                let sacrifices_source = rule.is_some_and(|rule| {
                    rule["costs"].as_array().is_some_and(|costs| {
                        costs.iter().any(|cost| {
                            value_kind(cost) == Some("sacrificePermanent")
                                && value_kind(&cost["permanent"]) == Some("self")
                        })
                    })
                });

                for (declaration_index, declaration) in declarations.into_iter().enumerate() {
                    if rule.is_some_and(|rule| {
                        !self.activated_sacrifice_costs_are_payable(
                            rule,
                            &player.id,
                            &permanent.instance_id,
                            &declaration.targets,
                        )
                    }) {
                        continue;
                    }
                    if rule.is_some_and(|rule| {
                        !rule["costs"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|cost| value_kind(cost) == Some("removeCounters"))
                            .all(|cost| {
                                Self::remove_counter_cost_is_payable(
                                    permanent,
                                    cost,
                                    &declaration.decisions,
                                )
                            })
                    }) {
                        continue;
                    }
                    if rule.is_some_and(|rule| {
                        !self.sacrificed_permanent_mana_symbols_match(
                            rule,
                            &declaration.targets,
                            &choice.symbols,
                        )
                    }) {
                        continue;
                    }
                    let mut excluded_sources = sacrifice_target_ids
                        .iter()
                        .filter_map(|target_id| declaration.targets.get(target_id))
                        .filter_map(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                            _ => None,
                        })
                        .collect::<BTreeSet<_>>();
                    if has_tap_cost || sacrifices_source {
                        excluded_sources.insert(permanent.instance_id.clone());
                    }
                    let Some(payment) = resolve_payment(
                        player_index,
                        &mana_costs,
                        &excluded_sources,
                        None,
                        Some(&permanent.definition),
                    )?
                    else {
                        continue;
                    };

                    let mana_label = choice
                        .symbols
                        .iter()
                        .map(|symbol| format!("{{{symbol}}}"))
                        .collect::<String>();
                    let cost_label = if choice.life_cost > 0 {
                        format!("Pay {} life, add {mana_label}", choice.life_cost)
                    } else {
                        format!("Add {mana_label}")
                    };
                    let simple_tap_label = has_tap_cost
                        && mana_costs.is_empty()
                        && sacrifice_target_ids.is_empty()
                        && !sacrifices_source;
                    let label = if simple_tap_label {
                        format!("Tap {}: {cost_label}", permanent.definition.name)
                    } else {
                        let mut activation_costs = mana_costs.clone();
                        if has_tap_cost {
                            activation_costs.push("{T}".to_string());
                        }
                        if !sacrifice_target_ids.is_empty() || sacrifices_source {
                            activation_costs.push("sacrifice".to_string());
                        }
                        format!(
                            "Activate {} ({}): Add {mana_label}",
                            permanent.definition.name,
                            activation_costs.join(", ")
                        )
                    };

                    let mut decisions = declaration.decisions;
                    decisions.insert("manaAbility".to_string(), Value::Bool(true));
                    decisions.insert("manaProduced".to_string(), json!(choice.symbols.clone()));
                    decisions.insert("lifePaid".to_string(), Value::from(choice.life_cost));
                    decisions.insert("tapSource".to_string(), Value::Bool(has_tap_cost));
                    decisions.insert("manaPayment".to_string(), mana_payment_decision(&payment));
                    if !payment.pool_indices.is_empty() {
                        decisions
                            .insert("manaPoolPayment".to_string(), json!(payment.pool_indices));
                    }
                    granted_mana_program::insert_origin(choice.origin.as_ref(), &mut decisions);
                    if let Some(restriction) = choice.spend_restriction.clone() {
                        decisions.insert("manaSpendRestriction".to_string(), restriction);
                    }
                    actions.push(LegalAction {
                        id: format!(
                            "mana:{}:{}:{choice_index}:{declaration_index}",
                            permanent.instance_id,
                            choice.native_rule_index().map_or_else(
                                || {
                                    if matches!(
                                        choice.origin,
                                        Some(ManaProgramOrigin::Granted { .. })
                                    ) {
                                        "granted".to_string()
                                    } else {
                                        "basic".to_string()
                                    }
                                },
                                |index| index.to_string()
                            )
                        ),
                        kind: ActionKind::ActivateAbility,
                        player_id: player.id.clone(),
                        label,
                        card_instance_id: Some(permanent.instance_id.clone()),
                        payment_sources: payment
                            .activations
                            .iter()
                            .map(|activation| activation.source_id.clone())
                            .collect(),
                        decisions,
                        targets: declaration.targets,
                        target_order: declaration.target_order,
                        attacker_id: None,
                        blocker_id: None,
                    });
                }
            }
        }
        Ok(actions)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_activation_actions_tests.rs"]
mod tests;
