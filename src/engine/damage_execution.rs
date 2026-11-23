//! Existing damage mutation/publication, using shared readonly preparation.
use super::*;

/// Damage can fail after the enclosing resolver has popped the stack or run
/// earlier actions. The shared resolution preflight owns the only recursive
/// walk and installs its existing whole-program checkpoint before either.
/// This conservative predicate also covers fallible scalar damage preparation.
pub(super) fn node_requires_checkpoint(effect: &Value) -> bool {
    value_kind(effect) == Some("dealDamage")
}
impl GameEngine {
    pub(super) fn deal_damage_amount_with_kind_and_origin(
        &mut self,
        target: TargetRef,
        amount: i32,
        source: &str,
        combat: bool,
        origin: QuantityOrigin,
        provider: Option<&mut dyn DecisionProvider>,
    ) -> Result<i32, EngineError> {
        self.deal_damage_amount_with_source_profile(
            target, amount, source, combat, origin, provider, None,
        )
    }
    /// As above, with the effective characteristics of a source the engine
    /// can no longer find in a zone (a resolving spell, CR 120.3).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn deal_damage_amount_with_source_profile(
        &mut self,
        target: TargetRef,
        amount: i32,
        source: &str,
        combat: bool,
        origin: QuantityOrigin,
        provider: Option<&mut dyn DecisionProvider>,
        source_profile: Option<&super::source_lki::ResolvedDamageSource>,
    ) -> Result<i32, EngineError> {
        if amount <= 0 {
            return Ok(0);
        }
        let mut quantity_provider = quantity_replacement::OptionalQuantityProvider(provider);
        let (target, events) = self.prepare_damage_redirection(target, amount, source);
        let frame = match self.prepare_damage_to_redirected_target(
            target,
            amount,
            source,
            combat,
            source_profile,
            events,
            &mut quantity_provider,
        )? {
            super::damage_preparation::PreparedDamage::Prevented {
                events,
                draws,
                counter_removals,
            } => {
                self.commit_prevented_damage(events, draws, counter_removals, None)?;
                return Ok(0);
            }
            super::damage_preparation::PreparedDamage::Ready(frame) => frame,
        };
        let super::damage_preparation::PreparedDamageFrame {
            target,
            amount,
            source_characteristics,
            events,
            counter_removals,
        } = frame;
        for removal in &counter_removals {
            if !self.commit_prepared_counter_removal(removal) {
                return Err(EngineError::new("prepared shield counter context changed"));
            }
        }
        self.publish_prepared_damage_events(events, None);
        let mut dealt_amount = 0;
        match target {
            TargetRef::Player { player_id } => {
                let player_index = self.player_index(&player_id)?;
                let infect = self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .chain(self.state.stack.iter().map(|object| &object.card))
                    .find(|card| card.instance_id == source)
                    .is_some_and(|card| self.permanent_has_keyword(card, "infect"));
                let toxic = if combat && amount > 0 {
                    self.state
                        .players
                        .iter()
                        .flat_map(|player| player.battlefield.iter())
                        .find(|card| card.instance_id == source)
                        .map(|card| self.permanent_keyword_count(card, "toxic", "count"))
                        .unwrap_or(0)
                } else {
                    0
                };
                let life_total_cannot_change = self.state.rule_modifiers.iter().any(|modifier| {
                    value_kind(modifier) == Some("lifeTotalCannotChange")
                        && modifier["playerId"].as_str() == Some(player_id.as_str())
                });
                if infect {
                    *self.state.players[player_index]
                        .counters
                        .entry("poison".to_string())
                        .or_default() += amount.max(0);
                } else if !life_total_cannot_change {
                    self.state.players[player_index].life -= amount;
                } else {
                    self.record_event(
                        "lifeChangePrevented",
                        Some(player_id.clone()),
                        Some(source.to_string()),
                        json!({ "amount": -amount, "reason": "lifeTotalCannotChange" }),
                    );
                }
                if toxic > 0 {
                    *self.state.players[player_index]
                        .counters
                        .entry("poison".to_string())
                        .or_default() += toxic;
                }
                dealt_amount = amount;
                self.record_event(
                    "damageDealt",
                    Some(player_id.clone()),
                    None,
                    json!({
                        "amount": amount,
                        "combat": combat,
                        "source": source,
                        "infect": infect,
                        "toxic": toxic,
                        "sourceControllerId": source_characteristics.as_ref().map(|value| &value.0),
                    }),
                );
                self.enqueue_source_damage_observers(
                    source,
                    source_characteristics
                        .as_ref()
                        .map(|value| value.0.as_str()),
                    source_damage_observation::DamageRecipientRef::Player(&player_id),
                    amount,
                    combat,
                );
                if let Some((source_controller, _, _, _)) = &source_characteristics {
                    self.enqueue_damage_to_opponent_triggers(
                        &source_controller.clone(),
                        &player_id,
                        amount,
                        source,
                        combat,
                    );
                    self.enqueue_player_damage_extra_observers(
                        source_controller,
                        &player_id,
                        source,
                    );
                }
            }
            TargetRef::Permanent { instance_id } => {
                if let Some((player_index, card_index)) = self.permanent_position(&instance_id) {
                    let retaliates = self.state.rule_modifiers.iter().any(|modifier| {
                        value_kind(modifier) == Some("destroyCreatureDamagingPermanent")
                            && modifier["permanentId"].as_str() == Some(instance_id.as_str())
                    });
                    let damaged_is_creature = self.permanent_is_creature(
                        &self.state.players[player_index].battlefield[card_index],
                    );
                    let damaged_is_planeswalker = has_card_type(
                        &self.state.players[player_index].battlefield[card_index].definition,
                        "Planeswalker",
                    );
                    let lethal_deathtouch = amount > 0
                        && source_characteristics
                            .as_ref()
                            .is_some_and(|(_, deathtouch, _, _)| *deathtouch);
                    let deals_with_wither = amount > 0
                        && damaged_is_creature
                        && source_characteristics
                            .as_ref()
                            .is_some_and(|(_, _, _, wither)| *wither);
                    if damaged_is_planeswalker {
                        self.remove_counters_from_permanent(
                            &instance_id,
                            "loyalty",
                            amount,
                            "damage",
                        );
                    }
                    if self.permanent_has_card_type(
                        &self.state.players[player_index].battlefield[card_index],
                        "Battle",
                    ) {
                        self.remove_counters_from_permanent(
                            &instance_id,
                            "defense",
                            amount,
                            "damage",
                        );
                    }
                    if deals_with_wither {
                        self.place_counters(
                            &instance_id,
                            "-1/-1",
                            amount,
                            origin,
                            source_characteristics.as_ref().map(|v| v.0.as_str()),
                            &mut quantity_provider,
                        )?;
                        if lethal_deathtouch
                            && let Some((player_index, card_index)) =
                                self.permanent_position(&instance_id)
                        {
                            let toughness = self.current_toughness(
                                &self.state.players[player_index].battlefield[card_index],
                            );
                            self.state.players[player_index].battlefield[card_index]
                                .damage_marked = toughness;
                        }
                    } else if damaged_is_creature && lethal_deathtouch {
                        let toughness = self.current_toughness(
                            &self.state.players[player_index].battlefield[card_index],
                        );
                        let permanent =
                            &mut self.state.players[player_index].battlefield[card_index];
                        permanent.damage_marked = permanent.damage_marked.max(toughness);
                    } else if damaged_is_creature {
                        self.state.players[player_index].battlefield[card_index].damage_marked +=
                            amount;
                    }
                    dealt_amount = amount;
                    self.record_event(
                        "damageDealt",
                        None,
                        Some(instance_id.clone()),
                        json!({
                            "amount": amount,
                            "combat": combat,
                            "source": source,
                            "sourceControllerId": source_characteristics.as_ref().map(|value| &value.0),
                        }),
                    );
                    if amount > 0
                        && let Some((damaged_player, damaged_index)) =
                            self.permanent_position(&instance_id)
                    {
                        let damaged =
                            self.state.players[damaged_player].battlefield[damaged_index].clone();
                        // One typed record of the damage family (damage_events).
                        let record = damage_events::DamageEventRecord {
                            source: source.to_string(),
                            source_controller: source_characteristics
                                .as_ref()
                                .map(|value| value.0.clone()),
                            recipient: damage_events::DamageRecipientRecord::Permanent(
                                instance_id.clone(),
                            ),
                            amount,
                            combat,
                            batch: self.current_damage_batch(),
                        };
                        self.enqueue_source_damage_observers(
                            source,
                            source_characteristics
                                .as_ref()
                                .map(|value| value.0.as_str()),
                            source_damage_observation::DamageRecipientRef::Permanent(&damaged),
                            amount,
                            combat,
                        );
                        self.check_quantity_failure()?;
                        let trigger_sources = self
                            .state
                            .players
                            .iter()
                            .flat_map(|player| player.battlefield.iter().cloned())
                            .collect::<Vec<_>>();
                        let decisions = BTreeMap::from([
                            ("damageAmount".to_string(), Value::from(amount)),
                            (
                                "triggeringPermanentId".to_string(),
                                Value::String(instance_id.clone()),
                            ),
                            (
                                "triggeringPermanentController".to_string(),
                                Value::String(damaged.controller.clone()),
                            ),
                        ]);
                        for ability_source in trigger_sources {
                            let rules = self
                                .effective_rules(&ability_source)
                                .iter()
                                .filter(|rule| {
                                    triggered_rule_supported(rule)
                                        && self.rule_active_for_source(&ability_source, rule)
                                        && self
                                            .rule_matches_damage_record(
                                                rule,
                                                damage_events::DamageGroup::ControlledRecipient,
                                                &record,
                                                &ability_source,
                                            )
                                            .is_some()
                                })
                                .cloned()
                                .collect::<Vec<_>>();
                            for rule in rules {
                                self.put_triggered_ability_on_stack_with_decisions(
                                    &ability_source,
                                    rule,
                                    decisions.clone(),
                                );
                                self.check_quantity_failure()?;
                            }
                        }
                    }
                    let decisions = BTreeMap::from([
                        (
                            "triggeringPermanentId".to_string(),
                            Value::String(instance_id.clone()),
                        ),
                        ("damageAmount".to_string(), Value::from(amount)),
                    ]);
                    if let Some((damaged_player, damaged_index)) =
                        self.permanent_position(&instance_id)
                    {
                        let damaged =
                            self.state.players[damaged_player].battlefield[damaged_index].clone();
                        let rules = self
                            .effective_rules(&damaged)
                            .iter()
                            .filter(|rule| {
                                triggered_rule_supported(rule)
                                    && self
                                        .rule_matches_damage_record(
                                            rule,
                                            damage_events::DamageGroup::RecipientSelf,
                                            &damage_events::DamageEventRecord {
                                                source: source.to_string(),
                                                source_controller: None,
                                                recipient:
                                                    damage_events::DamageRecipientRecord::Permanent(
                                                        instance_id.clone(),
                                                    ),
                                                amount,
                                                combat,
                                                batch: None,
                                            },
                                            &damaged,
                                        )
                                        .is_some()
                            })
                            .cloned()
                            .collect::<Vec<_>>();
                        for rule in rules {
                            self.put_triggered_ability_on_stack_with_decisions(
                                &damaged,
                                rule,
                                decisions.clone(),
                            );
                            self.check_quantity_failure()?;
                        }
                    }
                    if combat
                        && amount > 0
                        && self.permanent_position(&instance_id).is_some_and(
                            |(damaged_player, damaged_index)| {
                                self.permanent_is_creature(
                                    &self.state.players[damaged_player].battlefield[damaged_index],
                                )
                            },
                        )
                    {
                        let exile_attachments = self
                            .battlefield_static_modifiers()
                            .into_iter()
                            .filter(|(attachment, modifier)| {
                                attachment.attached_to.as_deref() == Some(source)
                                    && value_kind(modifier)
                                        == Some("attachedCombatDamageExileDamagedCreature")
                            })
                            .map(|(attachment, _)| attachment)
                            .collect::<Vec<_>>();
                        for attachment in exile_attachments {
                            self.put_triggered_ability_on_stack_with_origin(
                                &attachment,
                                json!({
                                    "kind": "triggeredAbility",
                                    "source": { "kind": "self" },
                                    "effects": [{
                                        "kind": "exilePermanent",
                                        "permanent": { "kind": "triggeringPermanent" },
                                    }],
                                }),
                                decisions.clone(),
                                trigger_multiplicity::TriggerOrigin::Unproven,
                            );
                            self.check_quantity_failure()?;
                        }
                    }
                    if retaliates
                        && self.permanent_position(source).is_some_and(
                            |(source_player, source_index)| {
                                self.permanent_is_creature(
                                    &self.state.players[source_player].battlefield[source_index],
                                )
                            },
                        )
                    {
                        self.move_permanent_to_graveyard(source, "destroyed")?;
                    }
                }
            }
            TargetRef::Card { .. } => {}
            TargetRef::StackObject { .. } => {}
        }
        if dealt_amount > 0
            && let Some((controller, _, true, _)) = source_characteristics
        {
            self.gain_life(&controller, dealt_amount)?;
        }
        Ok(dealt_amount)
    }
}
