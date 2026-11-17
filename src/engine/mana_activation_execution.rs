//! Execute a declared mana ability without using the gameplay stack.
//! Cost payment precedes executable production; immediate source scopes retain
//! departure-time LKI through the existing shared exit hooks.
use super::*;

impl GameEngine {
    pub(super) fn apply_mana_ability_action<P: DecisionProvider>(
        &mut self,
        player_index: usize,
        action: &LegalAction,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let instance_id = action
            .card_instance_id
            .as_deref()
            .ok_or_else(|| EngineError::new("mana ability has no source"))?;
        let Some((controller_index, card_index)) = self.permanent_position(instance_id) else {
            return Err(EngineError::new(
                "mana ability source is not on the battlefield",
            ));
        };
        if self.state.players[controller_index].battlefield[card_index].controller
            != self.state.players[player_index].id
        {
            return Err(EngineError::new(
                "player does not control mana ability source",
            ));
        }
        let source = self.state.players[controller_index].battlefield[card_index].clone();
        if let Some(identity) = action
            .decisions
            .get("manaSourceIncarnation")
            .and_then(Value::as_str)
            && Self::tracked_zone_identity(&source) != Some(identity)
        {
            return Err(EngineError::new(
                "declared mana source incarnation is no longer current",
            ));
        }

        let requested_origin = granted_mana_program::declared_origin(action)?;
        let rule = self.resolve_mana_program(&source, requested_origin.as_ref())?;
        let has_tap_cost = rule.as_ref().map_or(true, |rule| {
            rule["costs"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|cost| value_kind(cost) == Some("tap"))
        });
        if has_tap_cost && self.permanent_cannot_pay_tap_symbol(&source) {
            return Err(EngineError::new("mana ability source cannot be tapped"));
        }
        let declared_output = action
            .decisions
            .get("manaDeclaredOutput")
            .unwrap_or(&action.decisions["manaProduced"]);
        let requested_symbols = declared_output
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let requested_restriction = action.decisions.get("manaSpendRestriction");
        let choice = self
            .try_checked_mana_activation_plan(&source, None, true, true)?
            .into_activation_choices()
            .into_iter()
            .find(|choice| {
                choice.origin == requested_origin
                    && choice.symbols == requested_symbols
                    && choice.spend_restriction.as_ref() == requested_restriction
            })
            .ok_or_else(|| EngineError::new("mana ability choice is no longer legal"))?;
        if rule.as_ref().is_some_and(|rule| {
            !self.sacrificed_permanent_mana_symbols_match(rule, &action.targets, &choice.symbols)
        }) {
            return Err(EngineError::new(
                "sacrificed permanent cannot produce the selected mana type",
            ));
        }
        let postcost = self.mana_production_requires_postcost(rule.as_ref());
        if postcost
            && rule
                .as_ref()
                .is_some_and(mana_postcost::source_history_required)
        {
            return self.with_immediate_source_context(action, &source, |engine, context| {
                engine.commit_declared_mana_activation(
                    player_index,
                    action,
                    &source,
                    rule.as_ref(),
                    &choice,
                    has_tap_cost,
                    Some(context),
                    provider,
                )
            });
        }
        self.commit_declared_mana_activation(
            player_index,
            action,
            &source,
            rule.as_ref(),
            &choice,
            has_tap_cost,
            None,
            provider,
        )
    }

    pub(super) fn commit_declared_mana_activation<P: DecisionProvider>(
        &mut self,
        player_index: usize,
        action: &LegalAction,
        source: &CardInstance,
        rule: Option<&Value>,
        declared: &ManaChoice,
        has_tap_cost: bool,
        context_index: Option<usize>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let tapped_sources = self.apply_declared_mana_activation_costs(
            mana_activation_costs::ManaActivationCostRequest {
                player_index,
                action,
                source_instance_id: &source.instance_id,
                rule,
                choice: declared,
                has_tap_cost,
            },
            provider,
        )?;
        self.check_quantity_failure()?;
        let runtime_source = match context_index {
            Some(index) => self.immediate_source_context(index)?,
            None => mana_postcost::immediate_runtime_source(action, source),
        };
        let choice = if self.mana_production_requires_postcost(rule) {
            let selected_symbols = action.decisions["manaProduced"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            self.select_postcost_mana_choice(rule, declared, &runtime_source, &selected_symbols)?
        } else {
            declared.clone()
        };
        let mut floating_restriction = choice.spend_restriction.clone();
        if choice.snow {
            let metadata = floating_restriction.get_or_insert_with(|| json!({}));
            if let Some(metadata) = metadata.as_object_mut() {
                metadata.insert("producedBySnowSource".to_string(), Value::Bool(true));
            }
        }
        let mut produced_symbols = choice.symbols.clone();
        if has_tap_cost && !produced_symbols.is_empty() {
            produced_symbols.extend(
                self.state
                    .rule_modifiers
                    .iter()
                    .filter(|modifier| {
                        value_kind(modifier) == Some("additionalManaOnLandTap")
                            && modifier["expiresAfterTurn"].as_u64()
                                == Some(u64::from(self.state.turn_number))
                            && matches_card_filter(&source.definition, &modifier["where"])
                            && modifier["playerId"]
                                .as_str()
                                .is_none_or(|player_id| player_id == source.controller)
                    })
                    .filter_map(|modifier| modifier["mana"].as_str().map(ToOwned::to_owned)),
            );
        }
        for symbol in &produced_symbols {
            self.state.players[player_index]
                .mana_pool
                .push(FloatingMana {
                    symbol: symbol.clone(),
                    spend_restriction: floating_restriction.clone(),
                });
        }
        if let Some(rule) = rule {
            self.execute_mana_followups_in_source_context(rule, action, &runtime_source, provider)?;
        }
        self.record_event(
            "manaAbilityActivated",
            Some(action.player_id.clone()),
            Some(source.instance_id.clone()),
            json!({
                "lifePaid": choice.life_cost,
                "mana": produced_symbols.clone(),
                "spendRestriction": choice.spend_restriction,
            }),
        );
        self.enqueue_mana_ability_activated_triggers(source, &produced_symbols);
        self.check_quantity_failure()?;
        for source in tapped_sources {
            self.enqueue_permanent_tapped_triggers(&source);
            self.check_quantity_failure()?;
        }
        Ok(())
    }
}
