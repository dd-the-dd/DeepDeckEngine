//! Generic nonmana resolution cost preparation and commitment. The caller owns transactions.
use super::*;
use crate::engine::mana_discovery_session::ManaDiscoverySession;

pub(in crate::engine) struct PreparedNonManaPayment {
    player_id: String,
    player_index: usize,
    cost: Value,
    /// The decline option first, then every way to pay.
    options: Vec<LegalAction>,
}

impl PreparedNonManaPayment {
    pub(super) fn payable(&self) -> bool {
        self.options
            .iter()
            .any(|option| option.kind != ActionKind::DeclinePayment)
    }

    pub(super) fn forbid_payment(&mut self) {
        self.options
            .retain(|option| option.kind == ActionKind::DeclinePayment);
    }

    pub(super) fn require_payment(&mut self) {
        self.options
            .retain(|option| option.kind != ActionKind::DeclinePayment);
    }
}

impl GameEngine {
    pub(super) fn prepare_resolution_nonmana_payment(
        &self,
        payer_id: &str,
        mut cost: Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<PreparedNonManaPayment, EngineError> {
        let player_id = payer_id.to_string();
        let player_index = self.player_index(&player_id)?;
        for field in ["count", "amount"] {
            if !cost[field].is_null() {
                let value = self.checked_runtime_numeric_expression(
                    &cost[field],
                    stack_object,
                    bindings,
                    decisions,
                    "numeric resolution cost quantity",
                )?;
                if value < 0 {
                    return Err(EngineError::new(
                        "resolution cost quantity must be nonnegative",
                    ));
                }
                cost[field] = json!(value);
            }
        }
        if value_kind(&cost["counter"]) == Some("decisionResult") {
            let id = cost["counter"]["decisionId"]
                .as_str()
                .ok_or_else(|| EngineError::new("cost counter binding has no id"))?;
            let counter = decisions
                .get(id)
                .and_then(Value::as_str)
                .ok_or_else(|| EngineError::new("cost counter binding is absent or not a name"))?
                .to_string();
            cost["counter"] = json!(counter);
        }
        let cost = &cost;
        let required_resource = match value_kind(cost) {
            Some("putCounters") => {
                self.card_cost_reference_id(&cost["permanent"], stack_object, bindings)?
            }
            Some("tap") => self.card_cost_reference_id(&cost["object"], stack_object, bindings)?,
            _ => None,
        };
        let mut options = vec![LegalAction {
            id: format!("optionalPayment:{}:decline", stack_object.id),
            kind: ActionKind::DeclinePayment,
            player_id: player_id.clone(),
            label: "Decline payment".to_string(),
            card_instance_id: Some(stack_object.card.instance_id.clone()),
            payment_sources: Vec::new(),
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
            target_order: Vec::new(),
            attacker_id: None,
            blocker_id: None,
        }];
        options.extend(self.prepare_card_cost_options(
            cost,
            &player_id,
            stack_object,
            bindings,
            decisions,
        )?);
        if value_kind(cost) == Some("payPlayerCounters") {
            let counter = cost["counter"].as_str().unwrap_or_default();
            let count = self
                .checked_runtime_numeric_expression(
                    &cost["count"],
                    stack_object,
                    bindings,
                    decisions,
                    "numeric resolution cost count",
                )?
                .max(0);
            if self.state.players[player_index]
                .counters
                .get(counter)
                .copied()
                .unwrap_or(0)
                >= count
            {
                options.push(LegalAction {
                    id: format!("optionalPayment:{}:pay", stack_object.id),
                    kind: ActionKind::ChooseResolution,
                    player_id: player_id.clone(),
                    label: format!("Pay {count} {counter} counter(s)"),
                    card_instance_id: Some(stack_object.card.instance_id.clone()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::from([(
                        "playerCounterPayment".to_string(),
                        json!({ "counter": counter, "count": count }),
                    )]),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                });
            }
        } else if value_kind(cost) == Some("putCounters") {
            let counter = cost["counter"].as_str().unwrap_or_default();
            let count = self
                .checked_runtime_numeric_expression(
                    &cost["count"],
                    stack_object,
                    bindings,
                    decisions,
                    "numeric resolution cost count",
                )?
                .max(0);
            for permanent in self.state.players[player_index]
                .battlefield
                .iter()
                .filter(|permanent| permanent.controller == player_id)
                .filter(|permanent| {
                    required_resource
                        .as_ref()
                        .is_none_or(|id| id == &permanent.instance_id)
                })
                .filter(|permanent| {
                    self.permanent_matches_effective_filter(permanent, &cost["where"])
                })
            {
                options.push(LegalAction {
                    id: format!(
                        "optionalPayment:{}:putCounters:{}",
                        stack_object.id, permanent.instance_id
                    ),
                    kind: ActionKind::ChooseResolution,
                    player_id: player_id.clone(),
                    label: format!(
                        "Put {count} {counter} counter(s) on {}",
                        permanent.definition.name
                    ),
                    card_instance_id: Some(stack_object.card.instance_id.clone()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::from([(
                        "costPutCounterPermanent".to_string(),
                        json!(permanent.instance_id),
                    )]),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                });
            }
        } else if value_kind(cost) == Some("removeCounters") {
            let count = self
                .checked_runtime_numeric_expression(
                    &cost["count"],
                    stack_object,
                    bindings,
                    decisions,
                    "numeric resolution cost count",
                )?
                .max(0);
            if let Some((source_player, source_index)) =
                self.permanent_position(&stack_object.card.instance_id)
            {
                let permanent = &self.state.players[source_player].battlefield[source_index];
                for (counter, available) in &permanent.counters {
                    if *available >= count
                        && (cost["counter"].as_str() == Some("any")
                            || cost["counter"].as_str() == Some(counter.as_str()))
                    {
                        options.push(LegalAction {
                            id: format!(
                                "optionalPayment:{}:removeCounter:{}",
                                stack_object.id, counter
                            ),
                            kind: ActionKind::ChooseResolution,
                            player_id: player_id.clone(),
                            label: format!("Remove {count} {counter} counter(s)"),
                            card_instance_id: Some(stack_object.card.instance_id.clone()),
                            payment_sources: Vec::new(),
                            decisions: BTreeMap::from([(
                                "permanentCounterPayment".to_string(),
                                json!({ "counter": counter, "count": count }),
                            )]),
                            targets: BTreeMap::new(),
                            target_order: Vec::new(),
                            attacker_id: None,
                            blocker_id: None,
                        });
                    }
                }
            }
        } else if value_kind(cost) == Some("tapPermanents") {
            let count = self
                .checked_runtime_numeric_expression(
                    &cost["count"],
                    stack_object,
                    bindings,
                    decisions,
                    "numeric resolution cost count",
                )?
                .max(0) as usize;
            let candidates = self.state.players[player_index]
                .battlefield
                .iter()
                .filter(|permanent| permanent.controller == player_id && !permanent.tapped)
                .filter(|permanent| {
                    cost["excludeSource"].as_bool() != Some(true)
                        || permanent.instance_id != stack_object.card.instance_id
                })
                .filter(|permanent| {
                    self.permanent_matches_effective_filter(permanent, &cost["where"])
                })
                .map(|permanent| permanent.instance_id.clone())
                .collect::<Vec<_>>();
            for (index, selection) in
                Self::resolution_selections(&candidates, count, MAX_DECLARATION_OPTIONS)
                    .into_iter()
                    .enumerate()
            {
                options.push(LegalAction {
                    id: format!("optionalPayment:{}:tapMany:{index}", stack_object.id),
                    kind: ActionKind::ChooseResolution,
                    player_id: player_id.clone(),
                    label: format!("Tap {count} permanents"),
                    card_instance_id: Some(stack_object.card.instance_id.clone()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::from([(
                        "tapPermanentPayments".to_string(),
                        Value::Array(selection.into_iter().map(Value::String).collect()),
                    )]),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                });
            }
        } else if value_kind(cost) == Some("tap") {
            for permanent in self.state.players[player_index]
                .battlefield
                .iter()
                .filter(|permanent| permanent.controller == player_id && !permanent.tapped)
                .filter(|permanent| {
                    required_resource
                        .as_ref()
                        .is_none_or(|id| id == &permanent.instance_id)
                })
                .filter(|permanent| {
                    cost["excludeSource"].as_bool() != Some(true)
                        || permanent.instance_id != stack_object.card.instance_id
                })
                .filter(|permanent| {
                    self.permanent_matches_effective_filter(permanent, &cost["where"])
                })
            {
                options.push(LegalAction {
                    id: format!(
                        "optionalPayment:{}:tap:{}",
                        stack_object.id, permanent.instance_id
                    ),
                    kind: ActionKind::ChooseResolution,
                    player_id: player_id.clone(),
                    label: format!("Tap {}", permanent.definition.name),
                    card_instance_id: Some(stack_object.card.instance_id.clone()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::from([(
                        "costTapPermanent".to_string(),
                        json!(permanent.instance_id),
                    )]),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                });
            }
        } else if value_kind(cost) == Some("payWaterbend") {
            let amount = self
                .checked_runtime_numeric_expression(
                    &cost["amount"],
                    stack_object,
                    bindings,
                    decisions,
                    "numeric resolution cost amount",
                )?
                .max(0) as usize;
            let waterbend_sources = self.state.players[player_index]
                .battlefield
                .iter()
                .filter(|permanent| {
                    !permanent.tapped
                        && (self.permanent_has_card_type(permanent, "Artifact")
                            || self.permanent_is_creature(permanent))
                })
                .take(amount)
                .map(|permanent| permanent.instance_id.clone())
                .collect::<Vec<_>>();
            let remainder = amount.saturating_sub(waterbend_sources.len());
            if let Some(payment) = ManaDiscoverySession::new(self).payment_for_mana_requirement(
                player_index,
                ManaRequirement {
                    generic: i32::try_from(remainder).map_err(|_| {
                        EngineError::new("waterbend amount exceeds supported integer range")
                    })?,
                    ..Default::default()
                },
                &waterbend_sources.iter().cloned().collect(),
                None,
            )? {
                let mut decisions = BTreeMap::from([
                    ("manaPayment".to_string(), mana_payment_decision(&payment)),
                    ("waterbendSources".to_string(), json!(waterbend_sources)),
                ]);
                if !payment.pool_indices.is_empty() {
                    decisions.insert("manaPoolPayment".to_string(), json!(payment.pool_indices));
                }
                let mut payment_sources = payment
                    .activations
                    .iter()
                    .map(|activation| activation.source_id.clone())
                    .collect::<Vec<_>>();
                payment_sources.extend(waterbend_sources);
                options.push(LegalAction {
                    id: format!("optionalPayment:{}:waterbend", stack_object.id),
                    kind: ActionKind::ChooseResolution,
                    player_id: player_id.clone(),
                    label: format!("Waterbend {amount}"),
                    card_instance_id: Some(stack_object.card.instance_id.clone()),
                    payment_sources,
                    decisions,
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                });
            }
        }

        Ok(PreparedNonManaPayment {
            player_id,
            player_index,
            cost: cost.clone(),
            options,
        })
    }

    pub(super) fn choose_and_commit_resolution_nonmana_payment<P: DecisionProvider>(
        &mut self,
        prepared: PreparedNonManaPayment,
        scope_id: Option<&str>,
        stack_object: &StackObject,
        provider: &mut P,
    ) -> Result<PaymentOutcome, EngineError> {
        let PreparedNonManaPayment {
            player_id,
            player_index,
            cost,
            options,
        } = prepared;
        let cost = &cost;
        let available = options
            .iter()
            .any(|option| option.kind != ActionKind::DeclinePayment);
        if options.is_empty() {
            return Err(EngineError::new(
                "a mandatory resolution cost payment is no longer payable",
            ));
        }
        let payment = self.choose_action(
            provider,
            EngineDecisionRequest {
                id: scope_id.map_or_else(
                    || format!("optionalPayment:{}", stack_object.id),
                    |id| format!("costPayment:{}:{id}", stack_object.id),
                ),
                kind: DecisionKind::ResolutionChoice,
                player_id: player_id.clone(),
                source_card: Some(stack_object.card.clone()),
                source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                choice: None,
                options,
            },
        )?;
        if payment.kind == ActionKind::DeclinePayment {
            return Ok(if available {
                PaymentOutcome::Declined
            } else {
                PaymentOutcome::Unavailable
            });
        }
        if self.commit_card_cost_payment(cost, &payment, stack_object, provider)? {
            return Ok(PaymentOutcome::Paid);
        }
        if let Some(counter_payment) = payment.decisions.get("playerCounterPayment") {
            let counter = counter_payment["counter"].as_str().unwrap_or_default();
            let count = integer_value(&counter_payment["count"]).unwrap_or(0).max(0);
            *self.state.players[player_index]
                .counters
                .entry(counter.to_string())
                .or_default() -= count;
        } else if let Some(counter_payment) = payment.decisions.get("permanentCounterPayment") {
            let counter = counter_payment["counter"].as_str().unwrap_or_default();
            let count = integer_value(&counter_payment["count"]).unwrap_or(0).max(0);
            if let Some((source_player, source_index)) =
                self.permanent_position(&stack_object.card.instance_id)
            {
                let current = self.state.players[source_player].battlefield[source_index]
                    .counters
                    .entry(counter.to_string())
                    .or_default();
                *current = current.saturating_sub(count);
            }
        } else if let Some(tap_payments) = payment.decisions.get("tapPermanentPayments") {
            let instance_ids = tap_payments
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            for instance_id in instance_ids {
                if let Some((tap_player, tap_index)) = self.permanent_position(&instance_id) {
                    self.state.players[tap_player].battlefield[tap_index].tapped = true;
                    let tapped = self.state.players[tap_player].battlefield[tap_index].clone();
                    self.enqueue_permanent_tapped_triggers(&tapped);
                }
            }
        } else if let Some(instance_id) = payment
            .decisions
            .get("costTapPermanent")
            .and_then(Value::as_str)
        {
            if let Some((tap_player, tap_index)) = self.permanent_position(instance_id) {
                self.state.players[tap_player].battlefield[tap_index].tapped = true;
                let tapped = self.state.players[tap_player].battlefield[tap_index].clone();
                self.enqueue_permanent_tapped_triggers(&tapped);
            }
        } else if let Some(instance_id) = payment
            .decisions
            .get("costPutCounterPermanent")
            .and_then(Value::as_str)
        {
            let instance_id = instance_id.to_string();
            let counter = cost["counter"].as_str().unwrap_or_default();
            let count = integer_value(&cost["count"]).unwrap_or(0).max(0);
            self.pay_counter_placement_cost(&instance_id, counter, count, &player_id, provider)?;
        } else {
            self.validate_generated_mana_payment(&payment)?;
            let tapped_sources = self.apply_payment(player_index, &payment)?;
            for source in tapped_sources {
                self.enqueue_permanent_tapped_triggers(&source);
            }
            let waterbend_ids = payment
                .decisions
                .get("waterbendSources")
                .unwrap_or(&Value::Null)
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            for instance_id in waterbend_ids {
                if let Some((source_player, source_index)) = self.permanent_position(&instance_id) {
                    self.state.players[source_player].battlefield[source_index].tapped = true;
                    let source =
                        self.state.players[source_player].battlefield[source_index].clone();
                    self.enqueue_permanent_tapped_triggers(&source);
                }
            }
        }

        if value_kind(cost) == Some("payWaterbend") {
            self.mark_bending_performed(
                &player_id,
                "waterbend",
                Some(stack_object.card.instance_id.clone()),
            );
        }
        Ok(PaymentOutcome::Paid)
    }
}
