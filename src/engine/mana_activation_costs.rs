//! Physical payment stage of a declared mana activation.
//! Eligibility and declarations stay with their caller. The caller must obtain
//! actual production only after this service has paid every cost. This extraction
//! preserves the existing mutation order; it does not fix post-cost projection.
use super::*;

pub(super) struct ManaActivationCostRequest<'a> {
    pub player_index: usize,
    pub action: &'a LegalAction,
    pub source_instance_id: &'a str,
    pub rule: Option<&'a Value>,
    pub choice: &'a ManaChoice,
    pub has_tap_cost: bool,
}

impl GameEngine {
    /// Returns the tap receipts needed by the existing trigger publisher.
    /// Providers and chosen objects are passed through unchanged; this method
    /// never chooses an arbitrary option for a hypothetical execution.
    pub(super) fn apply_declared_mana_activation_costs<P: DecisionProvider>(
        &mut self,
        request: ManaActivationCostRequest<'_>,
        provider: &mut P,
    ) -> Result<Vec<CardInstance>, EngineError> {
        let ManaActivationCostRequest {
            player_index,
            action,
            source_instance_id: instance_id,
            rule,
            choice,
            has_tap_cost,
        } = request;
        // The real declaration has already validated this source. Resolve its
        // stable identity at service entry rather than transport storage indices.
        let source_incarnation = self
            .permanent_position(instance_id)
            .map(|(p, c)| {
                Self::tracked_zone_identity(&self.state.players[p].battlefield[c])
                    .map(str::to_owned)
            })
            .ok_or_else(|| EngineError::new("mana ability source is not on the battlefield"))?;
        if self.state.players[player_index].life < choice.life_cost {
            return Err(EngineError::new(format!(
                "{} cannot pay {} life for a mana ability",
                self.state.players[player_index].id, choice.life_cost
            )));
        }

        let mut tapped_sources = self.apply_payment(player_index, action)?;
        if has_tap_cost && let Some(source) = self.tap_permanent(instance_id)? {
            tapped_sources.push(source);
        }
        self.state.players[player_index].life -= choice.life_cost;
        if let Some(rule) = &rule {
            for cost in rule["costs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|cost| value_kind(cost) == Some("removeCounters"))
            {
                let counter = Self::remove_counter_cost_name(cost, &action.decisions)
                    .ok_or_else(|| EngineError::new("mana ability counter cost has no choice"))?;
                let count = cost_payment::activated_cost_count(&cost["count"], &action.decisions)
                    .ok_or_else(|| {
                    EngineError::new("mana ability counter cost has no count")
                })?;
                // Nested mana payments can remove earlier storage entries. Never
                // carry battlefield vector indices across a payment mutation.
                let (controller_index, card_index) = self
                    .permanent_position(instance_id)
                    .filter(|(p, c)| {
                        Self::tracked_zone_identity(&self.state.players[*p].battlefield[*c])
                            == source_incarnation.as_deref()
                    })
                    .ok_or_else(|| {
                        EngineError::new("counter cost source incarnation changed during payment")
                    })?;
                let available = self.state.players[controller_index].battlefield[card_index]
                    .counters
                    .get(&counter)
                    .copied()
                    .unwrap_or(0);
                if available < count {
                    return Err(EngineError::new("mana ability counter cost cannot be paid"));
                }
                self.state.players[controller_index].battlefield[card_index]
                    .counters
                    .insert(counter, available - count);
            }
            self.pay_activated_counter_costs(action, rule, provider)?;
            // `{T}, Pay {E}: Add ...`: energy leaves the activating player.
            self.pay_activated_player_counter_costs(&action.player_id, rule)?;
            self.apply_activated_sacrifice_costs(action, rule)?;
            self.apply_activated_chosen_object_costs(action, rule)?;
            if let Some(limit) = rule.get("activationLimit")
                && let Some(flag) = granted_mana_program::mana_activation_limit_flag(
                    limit,
                    self.state.turn_number,
                    choice.origin.as_ref(),
                )
                && let Some((source_player, source_index)) = self.permanent_position(instance_id)
            {
                self.state.players[source_player].battlefield[source_index]
                    .flags
                    .insert(flag, true);
            }
        }
        self.drain_linked_exile_returns(provider)?;
        Ok(tapped_sources)
    }
}
