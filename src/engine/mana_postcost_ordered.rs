//! Ordered world-sensitive mana declarations and payment receipts.
//! A node's resources are the real pool/life after all preceding costs and
//! activations. Gross preview output is not treated as spendable net mana.
use super::*;

pub(super) struct OrderedManaChoice {
    pub source_id: String,
    pub choice: ManaChoice,
    pub declaration: LegalAction,
}

pub(super) struct OrderedManaState {
    pub available_mana: Vec<(usize, String, bool)>,
    pub available_mana_cant_be_countered_indices: Vec<usize>,
    pub available_life: i32,
    pub choices: Vec<OrderedManaChoice>,
    /// A provider-dependent producer is not executable without that choice,
    /// but it must not hide other complete alternatives or an already paid goal.
    pub deferred_branch_error: Option<EngineError>,
    /// Diagnostic/progress identity only. Event history affects rules; this is
    /// never permission to equate worlds using only resource counts.
    pub progress_key: String,
}

pub(super) fn is_ordered_payment(decisions: &BTreeMap<String, Value>) -> bool {
    decisions
        .get("manaPayment")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|entry| {
            value_kind(entry) == Some("manaPaymentMetadata")
                && entry["orderedManaPayment"].as_bool() == Some(true)
        })
}

/// Exact restriction policy shared with the ordinary pool allocator. Filtering
/// retains physical pool indices; it never compacts indices into a new pool.
pub(super) fn mana_pool_entry_can_pay(
    mana: &FloatingMana,
    spending_card: Option<&CardDefinition>,
    ability_source: Option<&CardDefinition>,
    spending_source_zone: Option<&str>,
) -> bool {
    mana.spend_restriction.as_ref().is_none_or(|restriction| {
        super::mana_retention::unit_attributes_only(restriction)
            || value_kind(restriction) == Some("firebendingMana")
            || spending_card.is_some_and(|card| {
                value_kind(restriction) == Some("castSpell")
                    && matches_card_filter(card, &restriction["where"])
            })
            || value_kind(restriction) == Some("castSpellOrActivateAbility")
                && spending_card
                    .or(ability_source)
                    .is_some_and(|card| matches_card_filter(card, &restriction["where"]))
            || value_kind(restriction) == Some("castSpellFromZone")
                && spending_source_zone == restriction["zone"].as_str()
            || super::mana_spending_policy::is_use_union(restriction)
                && super::mana_spending_policy::use_union_permits(
                    restriction,
                    spending_card,
                    ability_source,
                )
            || super::mana_spend_prohibition::is_cast_spell_prohibition(restriction)
                && super::mana_spend_prohibition::prohibition_permits(restriction, spending_card)
    })
}

impl GameEngine {
    /// Keep literal independent land production on its existing fast kernel.
    /// Dynamic values or costs capable of changing the world require ordered
    /// evaluation, even when an initial snapshot happens to look sufficient.
    pub(super) fn requires_ordered_mana_payment(&self, player_index: usize) -> bool {
        if self.mana_production_requires_postcost(None) {
            return true;
        }
        // Every granted producer is executed/revalidated in order, so earlier
        // costs cannot leave stale availability for a later payment activation.
        if self.has_active_granted_mana_programs() {
            return true;
        }
        self.state.players.get(player_index).is_some_and(|player| {
            player.battlefield.iter().any(|source| {
                source.definition.printed_rules().iter().any(|rule| {
                    value_kind(rule) == Some("manaAbility")
                        && self.mana_production_requires_postcost(Some(rule))
                })
            })
        })
    }

    pub(super) fn project_ordered_mana_prefix(
        &self,
        player_index: usize,
        prefix: &[ManaActivation],
        excluded_sources: &BTreeSet<String>,
        spending_card: Option<&CardDefinition>,
        ability_source: Option<&CardDefinition>,
        spending_source_zone: Option<&str>,
    ) -> Result<OrderedManaState, EngineError> {
        let mut shadow = self.clone();
        for activation in prefix {
            let declaration = activation.declaration.as_deref().ok_or_else(|| {
                EngineError::new("ordered mana prefix is missing its activation declaration")
            })?;
            shadow.apply_mana_ability_action(
                player_index,
                declaration,
                &mut mana_postcost_preview::UndeclaredPreviewChoice::default(),
            )?;
        }
        let player = shadow
            .state
            .players
            .get(player_index)
            .ok_or_else(|| EngineError::new("ordered mana projection player is missing"))?;
        let available_life = player.life;
        let available_mana = player
            .mana_pool
            .iter()
            .enumerate()
            .filter(|(_, mana)| {
                mana_pool_entry_can_pay(mana, spending_card, ability_source, spending_source_zone)
            })
            .map(|(index, mana)| {
                (
                    index,
                    mana.symbol.clone(),
                    mana.spend_restriction.as_ref().is_some_and(|restriction| {
                        restriction["producedBySnowSource"].as_bool() == Some(true)
                    }),
                )
            })
            .collect::<Vec<_>>();
        let available_mana_cant_be_countered_indices = available_mana
            .iter()
            .filter_map(|(index, _, _)| {
                player.mana_pool[*index]
                    .spend_restriction
                    .as_ref()
                    .is_some_and(|restriction| {
                        restriction["cantBeCountered"].as_bool() == Some(true)
                    })
                    .then_some(*index)
            })
            .collect();
        // The ordered kernel deliberately does not memoize worlds. Avoid a
        // full GameState serialization solely for an unused diagnostic key.
        let progress_key = String::new();
        let context =
            mana_projection_context::ManaProjectionContext::for_value_interpretation(&shadow);
        let declared =
            shadow.try_mana_activation_actions_with(
                player_index,
                &mut |source| {
                    if excluded_sources.contains(&source.instance_id) {
                        return Ok(Vec::new());
                    }
                    shadow.checked_mana_activation_plan_in_context(
                    source, spending_card, true, false, &context,
                ).map(mana_checked_discovery::CheckedManaActivationPlan::into_activation_choices)
                },
                &mut |rule, controller, source| {
                    shadow.try_declarations_for_activated_rule_with_budget(
                        rule,
                        controller,
                        source,
                        &mut |_, _| {
                            Err(EngineError::new(
                                "automatic mana declaration unexpectedly requires a budget choice",
                            ))
                        },
                    ).map_err(declaration_result::DeclarationDependencyFailure::into_engine_error)
                },
                &mut |index, costs, excluded, _, ability_source| {
                    // An activation cost must be funded by mana already present,
                    // not by a future activation hidden in a reverse aggregate.
                    shadow.try_plan_mana_payment(
                        index,
                        costs,
                        excluded,
                        None,
                        ability_source,
                        None,
                        &mut |_: &CardInstance, _: Option<&CardDefinition>| {
                            Ok::<Vec<ManaChoice>, EngineError>(Vec::new())
                        },
                    )
                },
            )?;
        let mut choices = Vec::new();
        let mut deferred_branch_error = None;
        let mut seen = BTreeSet::new();
        for action in declared {
            let source_id = action
                .card_instance_id
                .as_deref()
                .ok_or_else(|| EngineError::new("ordered mana declaration has no source"))?;
            let (p, c) = shadow
                .permanent_position(source_id)
                .ok_or_else(|| EngineError::new("ordered mana declaration source is absent"))?;
            let source = &shadow.state.players[p].battlefield[c];
            let origin = granted_mana_program::declared_origin(&action)?;
            let symbols = action.decisions["manaProduced"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            let declaration_choice = shadow
                .checked_mana_activation_plan_in_context(
                    source,
                    spending_card,
                    true,
                    false,
                    &context,
                )?
                .into_activation_choices()
                .into_iter()
                .find(|choice| {
                    choice.origin == origin
                        && choice.symbols == symbols
                        && choice.spend_restriction.as_ref()
                            == action.decisions.get("manaSpendRestriction")
                })
                .ok_or_else(|| {
                    EngineError::new("ordered mana declaration has no certified producer")
                })?;
            let postcost = match shadow
                .postcost_choices_for_declared_action(&action, &declaration_choice)?
            {
                mana_postcost_preview::PostcostManaPreview::Complete(choices) => choices,
                mana_postcost_preview::PostcostManaPreview::DeferredNeedsProvider {
                    request_id,
                } => {
                    if deferred_branch_error.is_none() {
                        deferred_branch_error = Some(EngineError::new(format!(
                            "automatic mana cost requires an unresolved provider decision: {request_id}",
                        )));
                    }
                    continue;
                }
            };
            for result in postcost {
                let mut declaration = action.clone();
                declaration
                    .decisions
                    .insert("manaDeclaredOutput".into(), json!(symbols));
                declaration
                    .decisions
                    .insert("manaProduced".into(), json!(result.symbols));
                if let Some(identity) = Self::tracked_zone_identity(source) {
                    declaration
                        .decisions
                        .insert("manaSourceIncarnation".into(), json!(identity));
                }
                let mut key = declaration.clone();
                key.id.clear();
                key.label.clear();
                key.decisions.remove("manaDeclaredOutput");
                let key = serde_json::to_string(&key)
                    .map_err(|error| EngineError::new(error.to_string()))?;
                if seen.insert(key) {
                    choices.push(OrderedManaChoice {
                        source_id: source_id.to_string(),
                        choice: result,
                        declaration,
                    });
                }
            }
        }
        Ok(OrderedManaState {
            available_mana,
            available_mana_cant_be_countered_indices,
            available_life,
            choices,
            deferred_branch_error,
            progress_key,
        })
    }
}

impl GameEngine {
    /// Public callers own the shared outer checkpoint. Ordered source actions
    /// pay/resolve in receipt order; final spell mana is consumed afterward.
    /// The old pure route keeps its historical pool-before ordering.
    pub(super) fn apply_ordered_mana_payment(
        &mut self,
        player_index: usize,
        action: &LegalAction,
    ) -> Result<Vec<CardInstance>, EngineError> {
        let entries = action
            .decisions
            .get("manaPayment")
            .and_then(Value::as_array)
            .ok_or_else(|| EngineError::new("ordered mana payment has no receipt program"))?;
        let mut phyrexian_life = 0_i32;
        let mut expected_pool_symbols = None;
        for entry in entries {
            match value_kind(entry) {
                Some("manaPaymentMetadata") => {
                    if expected_pool_symbols.is_some() {
                        return Err(EngineError::new(
                            "ordered mana payment has duplicate metadata",
                        ));
                    }
                    expected_pool_symbols = Some(entry["manaPoolSymbols"].clone());
                }
                Some("phyrexianManaPayment") => {
                    let amount = integer_value(&entry["lifePaid"])
                        .filter(|amount| *amount >= 0)
                        .ok_or_else(|| EngineError::new("invalid ordered phyrexian life amount"))?;
                    phyrexian_life = phyrexian_life.checked_add(amount).ok_or_else(|| {
                        EngineError::new("ordered phyrexian life payment overflow")
                    })?;
                }
                _ => {
                    let declaration = granted_mana_program::ordered_receipt_declaration(
                        entry,
                        &action.player_id,
                    )?;
                    self.apply_mana_ability_action(
                        player_index,
                        &declaration,
                        &mut mana_postcost_preview::UndeclaredPreviewChoice::default(),
                    )?;
                }
            }
        }
        let indices = action
            .decisions
            .get("manaPoolPayment")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or_else(|| EngineError::new("ordered pool payment has an invalid index"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if indices.iter().copied().collect::<BTreeSet<_>>().len() != indices.len() {
            return Err(EngineError::new(
                "ordered pool payment has duplicate indices",
            ));
        }
        let actual_symbols = indices
            .iter()
            .map(|index| {
                self.state.players[player_index]
                    .mana_pool
                    .get(*index)
                    .map(|mana| mana.symbol.clone())
                    .ok_or_else(|| EngineError::new("ordered pool payment is no longer available"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if expected_pool_symbols.as_ref() != Some(&json!(actual_symbols)) {
            return Err(EngineError::new(
                "ordered final pool symbols do not match the payment receipt",
            ));
        }
        if self.state.players[player_index].life < phyrexian_life {
            return Err(EngineError::new(
                "ordered final phyrexian life payment is no longer affordable",
            ));
        }
        let mut removal = indices;
        removal.sort_unstable();
        for index in removal.into_iter().rev() {
            self.state.players[player_index].mana_pool.remove(index);
        }
        if phyrexian_life > 0 {
            self.state.players[player_index].life -= phyrexian_life;
            self.record_event(
                "lifePaid",
                Some(action.player_id.clone()),
                action.card_instance_id.clone(),
                json!({"amount":phyrexian_life,"reason":"phyrexianMana"}),
            );
        }
        // Each immediate activation already published its tap receipt once.
        Ok(Vec::new())
    }
}
