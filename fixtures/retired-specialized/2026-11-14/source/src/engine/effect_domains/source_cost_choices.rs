//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("payManaOrSacrificeSource") => effect["manaCost"]
            .as_str()
            .is_some_and(|mana_cost| parse_mana_requirement(mana_cost).is_some()),
        Some("payCostOrMoveSource") => {
            matches!(
                value_kind(&effect["cost"]),
                Some("payMana" | "payLife" | "payPlayerCounters" | "sacrificePermanent")
            ) && cost_supported(&effect["cost"])
                && matches!(effect["otherwise"].as_str(), Some("destroy" | "sacrifice"))
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_source_cost_choices_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_source_cost_choices_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_source_cost_choices_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("payCostOrMoveSource") => {
                let source_id = stack_object.card.instance_id.clone();
                let Some((source_player_index, source_card_index)) =
                    self.permanent_position(&source_id)
                else {
                    return Ok(());
                };
                let player_id = self.state.players[source_player_index].battlefield
                    [source_card_index]
                    .controller
                    .clone();
                let player_index = self.player_index(&player_id)?;
                let cost = &effect["cost"];
                let mut options = vec![LegalAction {
                    id: format!("sourceCost:{}:decline", stack_object.id),
                    kind: ActionKind::DeclinePayment,
                    player_id: player_id.clone(),
                    label: format!("Decline payment and {} this permanent", effect["otherwise"]),
                    card_instance_id: Some(source_id.clone()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                }];
                if value_kind(cost) == Some("payMana") {
                    let mana_cost = cost["manaCost"].as_str().unwrap_or_default().to_string();
                    if let Some(payment) = self.payment_for_mana_costs(
                        player_index,
                        std::slice::from_ref(&mana_cost),
                        &BTreeSet::new(),
                        None,
                    ) {
                        options.push(LegalAction {
                            id: format!("sourceCost:{}:pay", stack_object.id),
                            kind: ActionKind::ChooseResolution,
                            player_id: player_id.clone(),
                            label: format!("Pay {mana_cost}"),
                            card_instance_id: Some(source_id.clone()),
                            payment_sources: payment
                                .activations
                                .iter()
                                .map(|activation| activation.source_id.clone())
                                .collect(),
                            decisions: BTreeMap::from([
                                ("manaPayment".to_string(), mana_payment_decision(&payment)),
                                ("manaPoolPayment".to_string(), json!(payment.pool_indices)),
                            ]),
                            targets: BTreeMap::new(),
                            target_order: Vec::new(),
                            attacker_id: None,
                            blocker_id: None,
                        });
                    }
                } else if value_kind(cost) == Some("payPlayerCounters") {
                    let counter = cost["counter"].as_str().unwrap_or_default();
                    let count = integer_value(&cost["count"]).unwrap_or(0).max(0);
                    if self.state.players[player_index]
                        .counters
                        .get(counter)
                        .copied()
                        .unwrap_or(0)
                        >= count
                    {
                        options.push(LegalAction {
                            id: format!("sourceCost:{}:pay", stack_object.id),
                            kind: ActionKind::ChooseResolution,
                            player_id: player_id.clone(),
                            label: format!("Pay {count} {counter} counter(s)"),
                            card_instance_id: Some(source_id.clone()),
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
                } else if value_kind(cost) == Some("payLife") {
                    let amount = self
                        .runtime_numeric_expression(
                            &cost["amount"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .unwrap_or(0)
                        .max(0);
                    if self.state.players[player_index].life >= amount {
                        options.push(LegalAction {
                            id: format!("sourceCost:{}:pay", stack_object.id),
                            kind: ActionKind::ChooseResolution,
                            player_id: player_id.clone(),
                            label: format!("Pay {amount} life"),
                            card_instance_id: Some(source_id.clone()),
                            payment_sources: Vec::new(),
                            decisions: BTreeMap::from([(
                                "lifePayment".to_string(),
                                Value::from(amount),
                            )]),
                            targets: BTreeMap::new(),
                            target_order: Vec::new(),
                            attacker_id: None,
                            blocker_id: None,
                        });
                    }
                } else if value_kind(cost) == Some("sacrificePermanent") {
                    options.extend(
                        self.state.players[player_index]
                            .battlefield
                            .iter()
                            .filter(|permanent| {
                                permanent.instance_id != source_id
                                    && matches_permanent_filter(permanent, &cost["where"])
                            })
                            .map(|permanent| LegalAction {
                                id: format!(
                                    "sourceCost:{}:sacrifice:{}",
                                    stack_object.id, permanent.instance_id
                                ),
                                kind: ActionKind::ChooseResolution,
                                player_id: player_id.clone(),
                                label: format!("Sacrifice {}", permanent.definition.name),
                                card_instance_id: Some(permanent.instance_id.clone()),
                                payment_sources: Vec::new(),
                                decisions: BTreeMap::new(),
                                targets: BTreeMap::new(),
                                target_order: Vec::new(),
                                attacker_id: None,
                                blocker_id: None,
                            }),
                    );
                }
                let choice = self.choose_action(
                    provider,
                    EngineDecisionRequest {
                        id: format!("sourceCostPayment:{}", stack_object.id),
                        kind: DecisionKind::ResolutionChoice,
                        player_id: player_id.clone(),
                        source_card: Some(stack_object.card.clone()),
                        source_card_instance_id: Some(source_id.clone()),
                        choice: None,
                        options,
                    },
                )?;
                if choice.kind == ActionKind::DeclinePayment {
                    if effect["otherwise"].as_str() == Some("destroy") {
                        self.destroy_permanent_from_stack(&source_id, stack_object)?;
                    } else {
                        self.move_permanent_to_graveyard(&source_id, "sacrificed")?;
                    }
                } else if let Some(amount) = choice
                    .decisions
                    .get("lifePayment")
                    .and_then(Value::as_i64)
                    .and_then(|amount| i32::try_from(amount).ok())
                {
                    self.lose_life(&player_id, amount.max(0), &source_id)?;
                } else if let Some(payment) = choice.decisions.get("playerCounterPayment") {
                    let counter = payment["counter"].as_str().unwrap_or_default();
                    let count = integer_value(&payment["count"]).unwrap_or(0).max(0);
                    *self.state.players[player_index]
                        .counters
                        .entry(counter.to_string())
                        .or_default() -= count;
                    self.record_event(
                        "playerCountersPaid",
                        Some(player_id),
                        Some(source_id),
                        json!({ "counter": counter, "count": count }),
                    );
                } else if value_kind(cost) == Some("sacrificePermanent") {
                    let sacrificed_id = choice.card_instance_id.as_deref().ok_or_else(|| {
                        EngineError::new("sacrifice payment has no selected permanent")
                    })?;
                    self.move_permanent_to_graveyard(sacrificed_id, "sacrificed")?;
                } else {
                    let tapped_sources = self.apply_payment(player_index, &choice)?;
                    for source in tapped_sources {
                        self.enqueue_permanent_tapped_triggers(&source);
                    }
                }
            }
            Some("payManaOrSacrificeSource") => {
                let source_id = stack_object.card.instance_id.clone();
                let Some((source_player_index, source_card_index)) =
                    self.permanent_position(&source_id)
                else {
                    return Ok(());
                };
                let player_id = self.state.players[source_player_index].battlefield
                    [source_card_index]
                    .controller
                    .clone();
                self.state.players[source_player_index].battlefield[source_card_index]
                    .flags
                    .insert("echoDue".to_string(), false);
                let mana_cost = effect["manaCost"].as_str().unwrap_or_default().to_string();
                let player_index = self.player_index(&player_id)?;
                let payment = self.payment_for_mana_costs(
                    player_index,
                    std::slice::from_ref(&mana_cost),
                    &BTreeSet::new(),
                    None,
                );
                let mut options = vec![LegalAction {
                    id: format!("echo:{}:decline", stack_object.id),
                    kind: ActionKind::DeclinePayment,
                    player_id: player_id.clone(),
                    label: format!("Decline echo {mana_cost}"),
                    card_instance_id: Some(source_id.clone()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                }];
                if let Some(payment) = payment {
                    options.push(LegalAction {
                        id: format!("echo:{}:pay", stack_object.id),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: format!("Pay echo {mana_cost}"),
                        card_instance_id: Some(source_id.clone()),
                        payment_sources: payment
                            .activations
                            .iter()
                            .map(|activation| activation.source_id.clone())
                            .collect(),
                        decisions: BTreeMap::from([
                            ("manaPayment".to_string(), mana_payment_decision(&payment)),
                            ("manaPoolPayment".to_string(), json!(payment.pool_indices)),
                        ]),
                        targets: BTreeMap::new(),
                        target_order: Vec::new(),
                        attacker_id: None,
                        blocker_id: None,
                    });
                }
                let choice = self.choose_action(
                    provider,
                    EngineDecisionRequest {
                        id: format!("echoPayment:{}", stack_object.id),
                        kind: DecisionKind::ResolutionChoice,
                        player_id: player_id.clone(),
                        source_card: Some(stack_object.card.clone()),
                        source_card_instance_id: Some(source_id.clone()),
                        choice: None,
                        options,
                    },
                )?;
                if choice.kind == ActionKind::DeclinePayment {
                    self.move_permanent_to_graveyard(&source_id, "sacrificed")?;
                } else {
                    let tapped_sources = self.apply_payment(player_index, &choice)?;
                    for source in tapped_sources {
                        self.enqueue_permanent_tapped_triggers(&source);
                    }
                    self.record_event(
                        "echoPaid",
                        Some(player_id),
                        Some(source_id),
                        json!({ "manaCost": mana_cost }),
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
