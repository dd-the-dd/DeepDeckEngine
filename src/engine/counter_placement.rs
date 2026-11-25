//! Shared counter-placement preparation and publication.
//! Replacement order is resolved with the real provider before mutation.

use super::quantity_replacement::{QuantityEvent, QuantityOrigin, QuantityRecipient};
use super::*;

impl GameEngine {
    pub(super) fn counter_placement_is_prohibited(
        &self,
        permanent: &CardInstance,
        counter: &str,
    ) -> bool {
        self.static_counter_prohibited(permanent, counter)
            || (counter == "-1/-1"
                && self
                    .battlefield_static_modifiers()
                    .iter()
                    .any(|(source, modifier)| {
                        source.controller == permanent.controller
                            && value_kind(modifier) == Some("preventMinusOneCounters")
                            && self
                                .permanent_matches_effective_filter(permanent, &modifier["where"])
                    }))
    }

    pub(super) fn entering_counter_quantity<P: DecisionProvider>(
        &self,
        permanent: &CardInstance,
        counter: &str,
        count: i32,
        provider: &mut P,
    ) -> Result<i32, EngineError> {
        self.counter_quantity(permanent, counter, count, QuantityOrigin::Effect, provider)
    }

    pub(super) fn counter_quantity<P: DecisionProvider>(
        &self,
        permanent: &CardInstance,
        counter: &str,
        count: i32,
        origin: QuantityOrigin,
        provider: &mut P,
    ) -> Result<i32, EngineError> {
        if count <= 0 || self.counter_placement_is_prohibited(permanent, counter) {
            return Ok(0);
        }
        self.resolve_quantity_event(
            QuantityEvent {
                amount: count,
                origin,
                recipient: QuantityRecipient::Permanent { permanent, counter },
            },
            provider,
        )
        .map(|result| result.amount)
    }

    pub(super) fn place_counters<P: DecisionProvider>(
        &mut self,
        instance_id: &str,
        counter: &str,
        count: i32,
        origin: QuantityOrigin,
        placed_by: Option<&str>,
        provider: &mut P,
    ) -> Result<i32, EngineError> {
        self.place_counters_with_saga_policy(
            instance_id,
            counter,
            count,
            origin,
            placed_by,
            SagaChapterPolicy::CrossedThresholds,
            provider,
        )
    }

    pub(super) fn place_counters_with_saga_policy<P: DecisionProvider>(
        &mut self,
        instance_id: &str,
        counter: &str,
        count: i32,
        origin: QuantityOrigin,
        placed_by: Option<&str>,
        saga_policy: SagaChapterPolicy,
        provider: &mut P,
    ) -> Result<i32, EngineError> {
        if count <= 0 {
            return Ok(0);
        }
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return Ok(0);
        };
        let permanent = self.state.players[player_index].battlefield[card_index].clone();
        let actual = self.counter_quantity(&permanent, counter, count, origin, provider)?;
        self.commit_counter_placement(instance_id, counter, actual, placed_by, saga_policy)
    }

    /// Commit a quantity whose replacement program has already completed.
    /// Entry preparation also uses this publication boundary after insertion.
    pub(super) fn commit_counter_placement(
        &mut self,
        instance_id: &str,
        counter: &str,
        actual: i32,
        placed_by: Option<&str>,
        saga_policy: SagaChapterPolicy,
    ) -> Result<i32, EngineError> {
        if actual <= 0 {
            return Ok(0);
        }
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return Ok(0);
        };
        let permanent = self.state.players[player_index].battlefield[card_index].clone();
        let old_count = permanent.counters.get(counter).copied().unwrap_or(0);
        let controller = permanent.controller.clone();
        let placed_by = placed_by.unwrap_or(controller.as_str()).to_string();
        let new_count = old_count
            .checked_add(actual)
            .ok_or_else(|| EngineError::new("counter placement exceeds its integer bound"))?;
        self.state.players[player_index].battlefield[card_index]
            .counters
            .insert(counter.to_string(), new_count);
        self.publish_counter_placement(
            instance_id,
            counter,
            old_count,
            new_count,
            placed_by.as_str(),
            saga_policy,
        );
        Ok(actual)
    }

    pub(super) fn publish_counter_placement(
        &mut self,
        instance_id: &str,
        counter: &str,
        old_count: i32,
        new_count: i32,
        placed_by: &str,
        saga_policy: SagaChapterPolicy,
    ) {
        let actual = new_count.saturating_sub(old_count);
        if actual <= 0 {
            return;
        }
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return;
        };
        let controller = self.state.players[player_index].battlefield[card_index]
            .controller
            .clone();
        let placed_by = placed_by.to_string();
        let permanent = self.state.players[player_index].battlefield[card_index].clone();
        let new_count = permanent.counters.get(counter).copied().unwrap_or(0);
        self.record_event(
            "countersPlaced",
            Some(controller.clone()),
            Some(instance_id.to_string()),
            json!({
                "counter": counter,
                "count": actual,
                "oldCount": old_count,
                "newCount": new_count,
                "typeLine": permanent.definition.type_line,
                "placedByPlayerId": placed_by,
            }),
        );
        let triggers = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(&source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && value_kind(&rule["event"]) == Some("countersPlaced")
                            && (source.controller == controller
                                || rule["event"]["anyController"].as_bool() == Some(true))
                            && rule["event"].get("placedBy").is_none_or(|player| {
                                value_kind(player) == Some("controllerOf")
                                    && placed_by == source.controller
                            })
                            && (!rule["event"]["excludeSource"].as_bool().unwrap_or(false)
                                || source.instance_id != instance_id)
                            && rule["event"]
                                .get("counter")
                                .and_then(Value::as_str)
                                .is_none_or(|expected| expected == counter)
                            && rule["event"].get("object").is_none_or(|object| {
                                value_kind(object) == Some("self")
                                    && source.instance_id == instance_id
                            })
                            && rule["event"].get("resultingCount").is_none_or(|expected| {
                                integer_value(expected).is_some_and(|expected| {
                                    permanent.counters.get(counter).copied().unwrap_or_default()
                                        == expected as i32
                                })
                            })
                            && rule["event"].get("where").is_none_or(|filter| {
                                matches_card_filter(
                                    object_predicate::DefinitionView::for_permanent(&permanent),
                                    filter,
                                )
                            })
                    })
                    .cloned()
                    .map(|rule| (source.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let decisions = BTreeMap::from([
            ("counterCount".to_string(), Value::from(actual)),
            (
                "triggeringPermanentId".to_string(),
                Value::String(instance_id.to_string()),
            ),
            (
                "triggeringPermanentController".to_string(),
                Value::String(controller),
            ),
        ]);
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
        if counter == "lore" {
            self.enqueue_saga_chapters_for_lore_transition(
                &permanent,
                old_count,
                new_count,
                saga_policy,
            );
        }
    }
    /// Outer entry replacements finish before their contained counter event.
    /// None of the newcomers is live while this quantity is chosen.
    pub(super) fn apply_enter_replacements_with_decisions<P: DecisionProvider>(
        &mut self,
        card: &mut CardInstance,
        provider: &mut P,
        initial_decisions: &BTreeMap<String, Value>,
    ) -> Result<(), EngineError> {
        let original = card.clone();
        let extra_risk =
            if self.quantity_transaction_in_progress() || self.quantity_preflight_in_progress() {
                false
            } else {
                super::quantity_preflight::definition_may_enable_quantity(&card.definition)
                    || card
                        .printed_definition
                        .as_ref()
                        .is_some_and(super::quantity_preflight::definition_may_enable_quantity)
            };
        let result = self.with_quantity_preflight(extra_risk, |engine| {
            engine.apply_outer_enter_replacements_with_decisions(
                card,
                provider,
                initial_decisions,
            )?;
            if card
                .flags
                .get("enterGraveyardInstead")
                .copied()
                .unwrap_or(false)
            {
                return Ok(());
            }
            if card
                .flags
                .remove("additionalEnteringPlusOneCounter")
                .unwrap_or(false)
            {
                *card.counters.entry("+1/+1".to_string()).or_default() += 1;
            }
            // Initial loyalty/lore are replacement effects (CR306.5b/715.3).
            engine.prepare_saga_entry_lore(card);
            let requested = card.counters.clone();
            for (counter, count) in requested {
                let actual = engine.entering_counter_quantity(card, &counter, count, provider)?;
                card.counters.insert(counter.clone(), actual);
                if actual > 0 {
                    card.flags
                        .insert(format!("entryCounterPrepared:{counter}"), true);
                }
            }
            Ok(())
        });
        if result.is_err() {
            *card = original;
        }
        result
    }

    pub(super) fn publish_entering_counter_events(&mut self, source: &CardInstance) {
        let Some((p, c)) = self.permanent_position(&source.instance_id) else {
            return;
        };
        let counters = self.state.players[p].battlefield[c]
            .flags
            .keys()
            .filter_map(|flag| {
                flag.strip_prefix("entryCounterPrepared:")
                    .map(ToOwned::to_owned)
            })
            .collect::<Vec<_>>();
        let read_ahead = self.state.players[p].battlefield[c]
            .flags
            .remove("sagaEntryReadAheadPrepared")
            .unwrap_or(false);
        for counter in counters {
            self.state.players[p].battlefield[c]
                .flags
                .remove(&format!("entryCounterPrepared:{counter}"));
            let count = self.state.players[p].battlefield[c]
                .counters
                .get(&counter)
                .copied()
                .unwrap_or(0);
            let policy = if counter == "lore" && read_ahead {
                SagaChapterPolicy::ReadAheadSelectedOnly(count)
            } else {
                SagaChapterPolicy::CrossedThresholds
            };
            self.publish_counter_placement(
                &source.instance_id,
                &counter,
                0,
                count,
                &source.controller,
                policy,
            );
        }
    }
    pub(super) fn pay_counter_placement_cost<P: DecisionProvider>(
        &mut self,
        id: &str,
        counter: &str,
        count: i32,
        payer: &str,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let (p, c) = self
            .permanent_position(id)
            .ok_or_else(|| EngineError::new("counter cost permanent left the battlefield"))?;
        if count <= 0
            || self.counter_placement_is_prohibited(&self.state.players[p].battlefield[c], counter)
        {
            return Err(EngineError::new("counter cost placement is prohibited"));
        }
        self.place_counters(
            id,
            counter,
            count,
            QuantityOrigin::Cost,
            Some(payer),
            provider,
        )?;
        Ok(())
    }
    /// Existing providerless private APIs remain strict: an ambiguous order is
    /// an error, never an implicit first choice. Normal effects pass a provider.
    pub(super) fn deal_damage_amount_with_kind(
        &mut self,
        target: TargetRef,
        amount: i32,
        source: &str,
        combat: bool,
    ) -> Result<i32, EngineError> {
        let origin = if combat {
            QuantityOrigin::RuleAction
        } else {
            QuantityOrigin::Effect
        };
        self.deal_damage_amount_with_kind_and_origin(target, amount, source, combat, origin, None)
    }
    pub(super) fn deal_damage_amount_from_effect<P: DecisionProvider>(
        &mut self,
        target: TargetRef,
        amount: i32,
        source: &str,
        provider: &mut P,
    ) -> Result<i32, EngineError> {
        self.deal_damage_amount_with_kind_and_origin(
            target,
            amount,
            source,
            false,
            QuantityOrigin::Effect,
            Some(provider),
        )
    }
    pub(super) fn deal_damage_from_effect<P: DecisionProvider>(
        &mut self,
        target: TargetRef,
        amount: i32,
        source: &str,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        self.deal_damage_amount_from_effect(target, amount, source, provider)
            .map(|_| ())
    }
    #[cfg(test)]
    pub(super) fn put_counters_on_permanent(&mut self, id: &str, counter: &str, count: i32) -> i32 {
        self.put_counters_on_permanent_by(id, counter, count, None)
    }
    #[cfg(test)]
    pub(super) fn put_counters_on_permanent_by(
        &mut self,
        id: &str,
        counter: &str,
        count: i32,
        placed_by: Option<&str>,
    ) -> i32 {
        let mut provider = super::quantity_replacement::OptionalQuantityProvider(None);
        self.place_counters(
            id,
            counter,
            count,
            QuantityOrigin::Effect,
            placed_by,
            &mut provider,
        )
        .expect("private providerless test placement requires a deterministic replacement")
    }
    #[cfg(test)]
    pub(super) fn apply_escalate_costs(
        &mut self,
        action: &LegalAction,
        card: &CardDefinition,
    ) -> Result<(), EngineError> {
        self.apply_escalate_costs_with_provider(
            action,
            card,
            &mut super::quantity_replacement::OptionalQuantityProvider(None),
        )
    }
    /// Prepare every recipient before committing any part of one plural
    /// counter action. Triggers therefore see the entire placement event.
    pub(super) fn place_counters_batch<P: DecisionProvider>(
        &mut self,
        ids: Vec<String>,
        counter: &str,
        count: i32,
        origin: QuantityOrigin,
        placed_by: Option<&str>,
        provider: &mut P,
    ) -> Result<Vec<(String, i32)>, EngineError> {
        if count <= 0 {
            return Ok(Vec::new());
        }
        let prior_sequence = self.branching_decision_sequence.get();
        let prepared = (|| {
            let mut prepared = Vec::new();
            let mut seen = BTreeSet::new();
            for id in ids {
                if !seen.insert(id.clone()) {
                    continue;
                }
                let Some((p, c)) = self.permanent_position(&id) else {
                    continue;
                };
                let permanent = self.state.players[p].battlefield[c].clone();
                let actual = self.counter_quantity(&permanent, counter, count, origin, provider)?;
                let old = permanent.counters.get(counter).copied().unwrap_or(0);
                let new = old.checked_add(actual).ok_or_else(|| {
                    EngineError::new("counter placement exceeds its integer bound")
                })?;
                prepared.push((id, actual, old, new, permanent.controller));
            }
            Ok::<_, EngineError>(prepared)
        })();
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.branching_decision_sequence.set(prior_sequence);
                return Err(error);
            }
        };
        for (id, actual, _, new, _) in &prepared {
            if *actual > 0
                && let Some((p, c)) = self.permanent_position(id)
            {
                self.state.players[p].battlefield[c]
                    .counters
                    .insert(counter.to_string(), *new);
            }
        }
        for (id, actual, old, new, controller) in &prepared {
            if *actual > 0 {
                self.publish_counter_placement(
                    id,
                    counter,
                    *old,
                    *new,
                    placed_by.unwrap_or(controller),
                    SagaChapterPolicy::CrossedThresholds,
                );
            }
        }
        Ok(prepared
            .into_iter()
            .map(|(id, actual, _, _, _)| (id, actual))
            .collect())
    }
    /// One counter-cost adapter for battlefield, hand/graveyard and mana
    /// activation parents. The source's zone never changes the recipient.
    pub(super) fn pay_activated_counter_costs<P: DecisionProvider>(
        &mut self,
        action: &LegalAction,
        rule: &Value,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        for cost in rule["costs"].as_array().into_iter().flatten() {
            match value_kind(cost) {
                Some("putCounters") => {
                    let id = cost["permanent"]["id"].as_str().ok_or_else(|| {
                        EngineError::new("counter activation cost has no object choice")
                    })?;
                    self.apply_put_counter_cost(action, cost, id, provider)?;
                }
                Some("putCountersOnSource") => {
                    let id = action
                        .card_instance_id
                        .as_deref()
                        .ok_or_else(|| EngineError::new("counter cost source is missing"))?;
                    let counter = cost["counter"]
                        .as_str()
                        .ok_or_else(|| EngineError::new("counter cost kind is absent"))?;
                    let count = integer_value(&cost["count"])
                        .ok_or_else(|| EngineError::new("counter cost count is absent"))?;
                    self.pay_counter_placement_cost(
                        id,
                        counter,
                        count,
                        &action.player_id,
                        provider,
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct PreparedCounterRemoval {
    pub instance_id: String,
    pub counter: String,
    pub before: i32,
    pub after: i32,
    pub actual: i32,
    pub controller: String,
    pub reason: String,
}
impl GameEngine {
    pub(super) fn prepare_counter_removal(
        &self,
        instance_id: &str,
        counter: &str,
        count: i32,
        reason: &str,
    ) -> Option<PreparedCounterRemoval> {
        let (p, c) = self.permanent_position(instance_id)?;
        let card = &self.state.players[p].battlefield[c];
        let before = card.counters.get(counter).copied().unwrap_or(0).max(0);
        let actual = before.min(count.max(0));
        if actual == 0 {
            return None;
        }
        Some(PreparedCounterRemoval {
            instance_id: instance_id.into(),
            counter: counter.into(),
            before,
            after: before - actual,
            actual,
            controller: card.controller.clone(),
            reason: reason.into(),
        })
    }
    pub(super) fn counter_removal_context_matches(&self, plan: &PreparedCounterRemoval) -> bool {
        self.permanent_position(&plan.instance_id)
            .is_some_and(|(p, c)| {
                self.state.players[p].battlefield[c]
                    .counters
                    .get(&plan.counter)
                    .copied()
                    .unwrap_or(0)
                    .max(0)
                    == plan.before
            })
    }
    pub(super) fn commit_prepared_counter_removal(
        &mut self,
        plan: &PreparedCounterRemoval,
    ) -> bool {
        if !self.counter_removal_context_matches(plan) {
            return false;
        }
        let Some((p, c)) = self.permanent_position(&plan.instance_id) else {
            return false;
        };
        if plan.after == 0 {
            self.state.players[p].battlefield[c]
                .counters
                .remove(&plan.counter);
        } else {
            self.state.players[p].battlefield[c]
                .counters
                .insert(plan.counter.clone(), plan.after);
        }
        true
    }
    pub(super) fn publish_prepared_counter_removal(&mut self, plan: &PreparedCounterRemoval) {
        self.record_event(
            "counterRemoved",
            Some(plan.controller.clone()),
            Some(plan.instance_id.clone()),
            plan.event_detail(),
        );
    }
}

impl PreparedCounterRemoval {
    pub(super) fn event_detail(&self) -> Value {
        json!({"counter":self.counter,"count":self.actual,"reason":self.reason})
    }
}
