//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("optionalPayCostCreateReflexiveTrigger") => {
            player_reference_supported(&effect["player"])
                && matches!(
                    value_kind(&effect["cost"]),
                    Some(
                        "payMana"
                            | "payPlayerCounters"
                            | "putCounters"
                            | "removeCounters"
                            | "tap"
                            | "tapPermanents"
                            | "payWaterbend"
                    )
                )
                && cost_supported(&effect["cost"])
                && triggered_rule_supported(&effect["ability"])
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_reflexive_cost_transactions_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_reflexive_cost_transactions_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_reflexive_cost_transactions_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("optionalPayCostCreateReflexiveTrigger") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let cost = &effect["cost"];
                let mut options = vec![LegalAction {
                    id: format!("reflexivePayment:{}:decline", stack_object.id),
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
                if value_kind(cost) == Some("payPlayerCounters") {
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
                            id: format!("reflexivePayment:{}:pay", stack_object.id),
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
                    let count = integer_value(&cost["count"]).unwrap_or(0).max(0);
                    for permanent in self.state.players[player_index]
                        .battlefield
                        .iter()
                        .filter(|permanent| permanent.controller == player_id)
                        .filter(|permanent| {
                            self.permanent_matches_effective_filter(permanent, &cost["where"])
                        })
                    {
                        options.push(LegalAction {
                            id: format!(
                                "reflexivePayment:{}:putCounters:{}",
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
                            decisions: BTreeMap::new(),
                            targets: BTreeMap::from([(
                                "counterPayment".to_string(),
                                TargetRef::Permanent {
                                    instance_id: permanent.instance_id.clone(),
                                },
                            )]),
                            target_order: vec!["counterPayment".to_string()],
                            attacker_id: None,
                            blocker_id: None,
                        });
                    }
                } else if value_kind(cost) == Some("removeCounters") {
                    let count = integer_value(&cost["count"]).unwrap_or(0).max(0);
                    if let Some((source_player, source_index)) =
                        self.permanent_position(&stack_object.card.instance_id)
                    {
                        let permanent =
                            &self.state.players[source_player].battlefield[source_index];
                        for (counter, available) in &permanent.counters {
                            if *available >= count
                                && (cost["counter"].as_str() == Some("any")
                                    || cost["counter"].as_str() == Some(counter.as_str()))
                            {
                                options.push(LegalAction {
                                    id: format!(
                                        "reflexivePayment:{}:removeCounter:{}",
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
                } else if value_kind(cost) == Some("tap") {
                    for permanent in self.state.players[player_index]
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
                    {
                        options.push(LegalAction {
                            id: format!(
                                "reflexivePayment:{}:tap:{}",
                                stack_object.id, permanent.instance_id
                            ),
                            kind: ActionKind::ChooseResolution,
                            player_id: player_id.clone(),
                            label: format!("Tap {}", permanent.definition.name),
                            card_instance_id: Some(stack_object.card.instance_id.clone()),
                            payment_sources: Vec::new(),
                            decisions: BTreeMap::new(),
                            targets: BTreeMap::from([(
                                "tapPayment".to_string(),
                                TargetRef::Permanent {
                                    instance_id: permanent.instance_id.clone(),
                                },
                            )]),
                            target_order: vec!["tapPayment".to_string()],
                            attacker_id: None,
                            blocker_id: None,
                        });
                    }
                } else if let Some(mana_cost) = cost["manaCost"].as_str()
                    && let Some(payment) = mana_discovery_session::ManaDiscoverySession::new(self)
                        .payment_for_mana_costs(
                        player_index,
                        &[mana_cost.to_string()],
                        &BTreeSet::new(),
                        None,
                    )?
                {
                    options.push(LegalAction {
                        id: format!("reflexivePayment:{}:pay", stack_object.id),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: format!("Pay {mana_cost}"),
                        card_instance_id: Some(stack_object.card.instance_id.clone()),
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
                let payment = self.choose_action(
                    provider,
                    EngineDecisionRequest {
                        id: format!("reflexivePayment:{}", stack_object.id),
                        kind: DecisionKind::ResolutionChoice,
                        player_id: player_id.clone(),
                        source_card: Some(stack_object.card.clone()),
                        source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                        choice: None,
                        options,
                    },
                )?;
                if payment.kind == ActionKind::DeclinePayment {
                    return Ok(());
                }
                if let Some(counter_payment) = payment.decisions.get("playerCounterPayment") {
                    let counter = counter_payment["counter"].as_str().unwrap_or_default();
                    let count = integer_value(&counter_payment["count"]).unwrap_or(0).max(0);
                    *self.state.players[player_index]
                        .counters
                        .entry(counter.to_string())
                        .or_default() -= count;
                } else if let Some(counter_payment) =
                    payment.decisions.get("permanentCounterPayment")
                {
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
                } else if let Some(TargetRef::Permanent { instance_id }) =
                    payment.targets.get("tapPayment")
                {
                    if let Some((tap_player, tap_index)) = self.permanent_position(instance_id) {
                        self.state.players[tap_player].battlefield[tap_index].tapped = true;
                        let tapped = self.state.players[tap_player].battlefield[tap_index].clone();
                        self.enqueue_permanent_tapped_triggers(&tapped);
                    }
                } else if let Some(TargetRef::Permanent { instance_id }) =
                    payment.targets.get("counterPayment")
                {
                    let instance_id = instance_id.clone();
                    let counter = cost["counter"].as_str().unwrap_or_default();
                    let count = integer_value(&cost["count"]).unwrap_or(0).max(0);
                    self.pay_counter_placement_cost(
                        &instance_id,
                        counter,
                        count,
                        &player_id,
                        provider,
                    )?;
                } else {
                    let tapped_sources = self.apply_payment(player_index, &payment)?;
                    for source in tapped_sources {
                        self.enqueue_permanent_tapped_triggers(&source);
                    }
                }

                let ability = effect["ability"].clone();
                let declarations = self
                    .declarations_for_triggered_rule(&ability, &player_id, &stack_object.card)?
                    .unwrap_or_default();
                if declarations.is_empty() {
                    return Ok(());
                }
                let declaration_options = declarations
                    .into_iter()
                    .enumerate()
                    .map(|(index, declaration)| LegalAction {
                        id: format!("reflexiveTargets:{}:{index}", stack_object.id),
                        kind: ActionKind::ChooseResolution,
                        player_id: player_id.clone(),
                        label: "Choose targets for the reflexive ability".to_string(),
                        card_instance_id: Some(stack_object.card.instance_id.clone()),
                        payment_sources: Vec::new(),
                        decisions: declaration.decisions,
                        targets: declaration.targets,
                        target_order: declaration.target_order,
                        attacker_id: None,
                        blocker_id: None,
                    })
                    .collect::<Vec<_>>();
                let target_choice = self.choose_action(
                    provider,
                    EngineDecisionRequest {
                        id: format!("reflexiveTargets:{}", stack_object.id),
                        kind: DecisionKind::ResolutionChoice,
                        player_id: player_id.clone(),
                        source_card: Some(stack_object.card.clone()),
                        source_card_instance_id: Some(stack_object.card.instance_id.clone()),
                        choice: None,
                        options: declaration_options,
                    },
                )?;
                self.stack_sequence += 1;
                let reflexive_stack_id = format!("stack:{}", self.stack_sequence);
                self.state.stack.push(StackObject {
                    id: reflexive_stack_id.clone(),
                    controller: player_id.clone(),
                    card: stack_object.card.clone(),
                    cant_be_countered: false,
                    exile_on_leave_stack: false,
                    ability_kind: Some("triggeredAbility".to_string()),
                    ability_rule: Some(ability),
                    decisions: target_choice.decisions,
                    targets: target_choice.targets,
                });
                self.record_event(
                    "reflexiveTriggeredAbilityPutOnStack",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({
                        "cardName": stack_object.card.definition.name,
                        "objectKind": "triggeredAbility",
                        "stackId": reflexive_stack_id,
                    }),
                );
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
