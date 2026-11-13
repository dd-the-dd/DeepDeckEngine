//! Transactional non-mana cost primitives.
//!
//! Cost mutation is separated from rule-event publication so an activated
//! ability can be placed on the stack before triggers caused while paying its
//! costs are added above it.
//!
//! This module owns `payLife` and `sacrificePermanent` atomicity. Mana, tap,
//! discard, exile, counter, and return costs still use their existing engine
//! paths and are outside this transaction's rollback boundary.

use super::*;

#[derive(Clone, Debug)]
enum PreparedActivatedCost {
    PayLife { amount: i32 },
    SacrificePermanent { instance_id: String },
}

#[derive(Clone, Debug)]
pub(super) struct PreparedActivatedCostPayment {
    player_id: String,
    source_id: String,
    costs: Vec<PreparedActivatedCost>,
}

pub(super) struct CommittedActivatedCostPayment {
    events: Vec<CostDomainEvent>,
    deferred_stack_objects: Vec<StackObject>,
}

#[derive(Clone, Debug)]
pub(super) enum CostDomainEvent {
    LifeLost {
        player_id: String,
        source_id: String,
        amount: i32,
    },
    PermanentSacrificed {
        player_id: String,
        source_id: Option<String>,
        permanent: CardInstance,
    },
}

impl GameEngine {
    pub(super) fn player_can_pay_life_cost(&self, player_id: &str, amount: i32) -> bool {
        if amount < 0
            || self.state.rule_modifiers.iter().any(|modifier| {
                amount > 0
                    && value_kind(modifier) == Some("lifeTotalCannotChange")
                    && modifier["playerId"].as_str() == Some(player_id)
            })
        {
            return false;
        }
        self.player_index(player_id)
            .ok()
            .is_some_and(|index| self.state.players[index].life >= amount)
    }

    /// Computes the complete life commitment without mutating the source zone.
    /// Generation and transactional execution use the same numeric policy.
    pub(super) fn activated_life_cost_total(
        &self,
        rule: &Value,
        controller_id: &str,
        source: &CardInstance,
    ) -> Option<i32> {
        rule["costs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cost| value_kind(cost) == Some("payLife"))
            .try_fold(0_i32, |total, cost| {
                let amount = self.numeric_expression_with_source(
                    &cost["amount"],
                    controller_id,
                    Some(source),
                )?;
                (amount >= 0).then_some(())?;
                total.checked_add(amount)
            })
    }

    fn permanent_can_pay_sacrifice_cost(&self, instance_id: &str, controller_id: &str) -> bool {
        self.permanent_position(instance_id)
            .is_some_and(|(player_index, card_index)| {
                let permanent = &self.state.players[player_index].battlefield[card_index];
                permanent.controller == controller_id
                    && !self.permanent_has_static_modifier(permanent, "cantBeSacrificed")
            })
    }

    fn activated_sacrifice_instance_id(
        &self,
        action: &LegalAction,
        cost: &Value,
    ) -> Option<String> {
        match value_kind(&cost["permanent"]) {
            Some("self" | "abilitySource") => action.card_instance_id.clone(),
            Some("chosenTarget" | "chosenObject") => cost["permanent"]["id"]
                .as_str()
                .and_then(|target_id| action.targets.get(target_id))
                .and_then(|target| match target {
                    TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                    _ => None,
                }),
            _ => None,
        }
    }

    pub(super) fn activated_sacrifice_costs_are_payable(
        &self,
        rule: &Value,
        controller_id: &str,
        source_instance_id: &str,
        targets: &BTreeMap<String, TargetRef>,
    ) -> bool {
        let action = LegalAction {
            id: "cost-preflight".to_string(),
            kind: ActionKind::ActivateAbility,
            player_id: controller_id.to_string(),
            label: String::new(),
            card_instance_id: Some(source_instance_id.to_string()),
            payment_sources: Vec::new(),
            decisions: BTreeMap::new(),
            targets: targets.clone(),
            target_order: Vec::new(),
            attacker_id: None,
            blocker_id: None,
        };
        let mut selected = BTreeSet::new();
        rule["costs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cost| value_kind(cost) == Some("sacrificePermanent"))
            .all(|cost| {
                self.activated_sacrifice_instance_id(&action, cost)
                    .filter(|instance_id| selected.insert(instance_id.clone()))
                    .is_some_and(|instance_id| {
                        self.permanent_can_pay_sacrifice_cost(&instance_id, controller_id)
                    })
            })
    }

    pub(super) fn prepare_activated_cost_payment(
        &self,
        action: &LegalAction,
        rule: &Value,
    ) -> Result<PreparedActivatedCostPayment, EngineError> {
        let source_id = action
            .card_instance_id
            .clone()
            .ok_or_else(|| EngineError::new("activated cost source is missing"))?;
        let (source, payer) =
            if let Some((player_index, card_index)) = self.permanent_position(&source_id) {
                let source = &self.state.players[player_index].battlefield[card_index];
                (source, source.controller.as_str())
            } else {
                let source = self.card_outside_battlefield(&source_id).ok_or_else(|| {
                    EngineError::new("activated cost source is absent from its activation zone")
                })?;
                (source, source.owner.as_str())
            };
        if payer != action.player_id {
            return Err(EngineError::new(
                "activated cost source is not controlled by the payer",
            ));
        }

        let mut costs = Vec::new();
        let mut selected_sacrifices = BTreeSet::new();
        let total_life = self
            .activated_life_cost_total(rule, &action.player_id, source)
            .ok_or_else(|| {
                EngineError::new("activated life cost is not a bounded nonnegative amount")
            })?;
        for cost in rule["costs"].as_array().into_iter().flatten() {
            match value_kind(cost) {
                Some("payLife") => {
                    let amount = self
                        .numeric_expression_with_source(
                            &cost["amount"],
                            &action.player_id,
                            Some(source),
                        )
                        .ok_or_else(|| EngineError::new("activated life cost is not numeric"))?;
                    if amount < 0 {
                        return Err(EngineError::new("activated life cost cannot be negative"));
                    }
                    costs.push(PreparedActivatedCost::PayLife { amount });
                }
                Some("sacrificePermanent") => {
                    let instance_id = self
                        .activated_sacrifice_instance_id(action, cost)
                        .ok_or_else(|| {
                            EngineError::new("activated sacrifice cost has no permanent")
                        })?;
                    if !selected_sacrifices.insert(instance_id.clone()) {
                        return Err(EngineError::new(
                            "the same permanent cannot pay two sacrifice costs",
                        ));
                    }
                    if !self.permanent_can_pay_sacrifice_cost(&instance_id, &action.player_id) {
                        return Err(EngineError::new("activated sacrifice cost is not payable"));
                    }
                    costs.push(PreparedActivatedCost::SacrificePermanent { instance_id });
                }
                _ => {}
            }
        }
        let declared_life = action
            .decisions
            .get("activatedLifePayment")
            .and_then(Value::as_i64)
            .and_then(|amount| i32::try_from(amount).ok())
            .unwrap_or(0);
        if declared_life != total_life {
            return Err(EngineError::new(
                "activated life payment no longer matches its rule",
            ));
        }
        if !self.player_can_pay_life_cost(&action.player_id, total_life) {
            return Err(EngineError::new(
                "player cannot pay activated ability life cost",
            ));
        }
        Ok(PreparedActivatedCostPayment {
            player_id: action.player_id.clone(),
            source_id,
            costs,
        })
    }

    pub(super) fn commit_activated_cost_payment(
        &mut self,
        payment: &PreparedActivatedCostPayment,
    ) -> Result<CommittedActivatedCostPayment, EngineError> {
        if payment.costs.is_empty() {
            return Ok(CommittedActivatedCostPayment {
                events: Vec::new(),
                deferred_stack_objects: Vec::new(),
            });
        }
        // Validate the full transaction again immediately before its first
        // mutation. No rule event is published from this phase.
        let total_life = payment
            .costs
            .iter()
            .filter_map(|cost| match cost {
                PreparedActivatedCost::PayLife { amount } => Some(*amount),
                PreparedActivatedCost::SacrificePermanent { .. } => None,
            })
            .sum::<i32>();
        if !self.player_can_pay_life_cost(&payment.player_id, total_life)
            || !payment.costs.iter().all(|cost| match cost {
                PreparedActivatedCost::PayLife { .. } => true,
                PreparedActivatedCost::SacrificePermanent { instance_id } => {
                    self.permanent_can_pay_sacrifice_cost(instance_id, &payment.player_id)
                }
            })
        {
            return Err(EngineError::new(
                "activated cost transaction is no longer payable",
            ));
        }

        let state_before = self.state.clone();
        let stack_len_before = self.state.stack.len();
        let result: Result<Vec<CostDomainEvent>, EngineError> = (|| {
            let mut events = Vec::new();
            for cost in &payment.costs {
                match cost {
                    PreparedActivatedCost::PayLife { amount } => {
                        let player_index = self.player_index(&payment.player_id)?;
                        self.state.players[player_index].life -= amount;
                        if *amount > 0 {
                            events.push(CostDomainEvent::LifeLost {
                                player_id: payment.player_id.clone(),
                                source_id: payment.source_id.clone(),
                                amount: *amount,
                            });
                        }
                    }
                    PreparedActivatedCost::SacrificePermanent { instance_id } => {
                        events.push(self.commit_sacrifice_permanent(
                            instance_id,
                            Some(payment.source_id.clone()),
                        )?);
                    }
                }
            }
            Ok(events)
        })();
        if result.is_err() {
            self.state = state_before;
        }
        result.map(|events| CommittedActivatedCostPayment {
            events,
            deferred_stack_objects: self.state.stack.split_off(stack_len_before),
        })
    }

    /// Off-battlefield activation pays source movement before mana. When the
    /// same payment also contains life or sacrifice costs, a later refusal
    /// must restore the earlier source movement and mana payment as well.
    /// Ordinary source-only costs retain their existing inexpensive path.
    pub(super) fn with_activated_cost_rollback<T>(
        &mut self,
        payment: &PreparedActivatedCostPayment,
        work: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let previous = (!payment.costs.is_empty()).then(|| self.clone());
        let result = work(self);
        if result.is_err()
            && let Some(previous) = previous
        {
            *self = previous;
        }
        result
    }

    pub(super) fn publish_activated_cost_payment(
        &mut self,
        payment: CommittedActivatedCostPayment,
    ) {
        self.state.stack.extend(payment.deferred_stack_objects);
        self.publish_cost_domain_events(payment.events);
    }

    fn commit_sacrifice_permanent(
        &mut self,
        instance_id: &str,
        source_id: Option<String>,
    ) -> Result<CostDomainEvent, EngineError> {
        let (player_index, card_index) = self
            .permanent_position(instance_id)
            .ok_or_else(|| EngineError::new("sacrifice permanent left the battlefield"))?;
        let permanent = self.state.players[player_index].battlefield[card_index].clone();
        if self.permanent_has_static_modifier(&permanent, "cantBeSacrificed") {
            return Err(EngineError::new("permanent cannot be sacrificed"));
        }
        self.move_permanent_to_graveyard_internal(instance_id, "sacrificed")?;
        if self.permanent_position(instance_id).is_some() {
            return Err(EngineError::new("sacrifice did not move the permanent"));
        }
        Ok(CostDomainEvent::PermanentSacrificed {
            player_id: permanent.controller.clone(),
            source_id,
            permanent,
        })
    }

    pub(super) fn publish_cost_domain_events(&mut self, events: Vec<CostDomainEvent>) {
        for event in events {
            match event {
                CostDomainEvent::LifeLost {
                    player_id,
                    source_id,
                    amount,
                } => {
                    self.record_event(
                        "lifeLost",
                        Some(player_id.clone()),
                        Some(source_id.clone()),
                        json!({ "amount": amount, "source": source_id, "cause": "cost" }),
                    );
                    if amount > 0 {
                        self.enqueue_life_lost_triggers(&player_id, amount);
                    }
                    self.check_life_totals();
                }
                CostDomainEvent::PermanentSacrificed {
                    player_id,
                    source_id,
                    permanent,
                } => {
                    self.record_event(
                        "permanentSacrificed",
                        Some(player_id),
                        Some(permanent.instance_id.clone()),
                        json!({
                            "source": source_id,
                            "typeLine": permanent.definition.type_line.clone(),
                            "wasCreature": self.permanent_is_creature(&permanent),
                            "wasToken": permanent.definition.is_token,
                        }),
                    );
                    self.enqueue_permanent_sacrificed_triggers(&permanent);
                }
            }
        }
    }

    fn enqueue_permanent_sacrificed_triggers(&mut self, sacrificed: &CardInstance) {
        let mut sources = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter().cloned())
            .collect::<Vec<_>>();
        if !sources
            .iter()
            .any(|source| source.instance_id == sacrificed.instance_id)
        {
            sources.push(sacrificed.clone());
        }
        let decisions = BTreeMap::from([
            (
                "triggeringPermanentId".to_string(),
                Value::String(sacrificed.instance_id.clone()),
            ),
            (
                "triggeringPermanentController".to_string(),
                Value::String(sacrificed.controller.clone()),
            ),
            (
                "triggeringPermanentOwner".to_string(),
                Value::String(sacrificed.owner.clone()),
            ),
            (
                "triggeringPermanentWasToken".to_string(),
                Value::Bool(sacrificed.definition.is_token),
            ),
            (
                "triggerReason".to_string(),
                Value::String("sacrificed".to_string()),
            ),
        ]);
        let mut triggers = Vec::new();
        for source in sources {
            for rule in source.definition.rules.iter().filter(|rule| {
                triggered_rule_supported(rule) && self.rule_active_for_source(&source, rule)
            }) {
                let matches = triggered_event_variants(&rule["event"])
                    .into_iter()
                    .any(|event| match value_kind(event) {
                        Some("permanentSacrificed") => {
                            event
                                .get("player")
                                .is_none_or(|_| sacrificed.controller == source.controller)
                                && event.get("where").is_none_or(|where_filter| {
                                    matches_permanent_filter(sacrificed, where_filter)
                                })
                                && (!event["excludeSource"].as_bool().unwrap_or(false)
                                    || sacrificed.instance_id != source.instance_id)
                        }
                        Some("tokenCreatedOrSacrificed") => {
                            sacrificed.definition.is_token
                                && sacrificed.controller == source.controller
                        }
                        _ => false,
                    });
                if matches {
                    triggers.push((source.clone(), rule.clone()));
                }
            }
        }
        for (source, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions.clone());
        }
    }

    fn sacrifice_permanent_immediately(&mut self, instance_id: &str) -> Result<(), EngineError> {
        let Some((player_index, card_index)) = self.permanent_position(instance_id) else {
            return Ok(());
        };
        let permanent = &self.state.players[player_index].battlefield[card_index];
        if self.permanent_has_static_modifier(permanent, "cantBeSacrificed") {
            self.record_event(
                "sacrificeProhibited",
                None,
                Some(instance_id.to_string()),
                json!({}),
            );
            return Ok(());
        }
        let event = self.commit_sacrifice_permanent(instance_id, None)?;
        self.publish_cost_domain_events(vec![event]);
        Ok(())
    }

    pub(super) fn move_permanent_to_graveyard(
        &mut self,
        instance_id: &str,
        reason: &str,
    ) -> Result<(), EngineError> {
        if reason == "sacrificed" {
            self.sacrifice_permanent_immediately(instance_id)
        } else {
            self.move_permanent_to_graveyard_internal(instance_id, reason)
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "cost_payment_tests.rs"]
mod tests;
