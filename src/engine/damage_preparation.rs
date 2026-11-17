//! Shared readonly damage redirection/modification/prevention preparation.
//! Actual mutation and publication are separate callers' phases.
use super::counter_placement::PreparedCounterRemoval;
use super::*;

pub(super) struct DeferredDamageEvent {
    pub kind: &'static str,
    pub player_id: Option<String>,
    pub card_id: Option<String>,
    pub detail: Value,
}
#[derive(Default)]
struct DamageEventCollector {
    records: Vec<DeferredDamageEvent>,
}
impl DamageEventCollector {
    fn record_event(
        &mut self,
        kind: &'static str,
        player_id: Option<String>,
        card_id: Option<String>,
        detail: Value,
    ) {
        self.records.push(DeferredDamageEvent {
            kind,
            player_id,
            card_id,
            detail,
        });
    }
}
pub(super) struct PreparedDamageFrame {
    pub target: TargetRef,
    pub amount: i32,
    pub source_characteristics: Option<(String, bool, bool, bool)>,
    pub events: Vec<DeferredDamageEvent>,
    pub counter_removals: Vec<PreparedCounterRemoval>,
}
pub(super) enum PreparedDamage {
    Prevented {
        events: Vec<DeferredDamageEvent>,
        draws: Vec<(String, usize)>,
        counter_removals: Vec<PreparedCounterRemoval>,
    },
    Ready(PreparedDamageFrame),
}
impl GameEngine {
    pub(super) fn prepare_damage_redirection(
        &self,
        target: TargetRef,
        amount: i32,
        source: &str,
    ) -> (TargetRef, Vec<DeferredDamageEvent>) {
        let mut events = DamageEventCollector::default();
        let target = if let TargetRef::Player { player_id } = &target {
            let redirected_to = self
                .state
                .players
                .iter()
                .flat_map(|player| player.battlefield.iter())
                .find_map(|aura| {
                    aura.definition.rules.iter().find_map(|rule| {
                        (value_kind(rule) == Some("staticAbility")
                            && rule["modifiers"].as_array().is_some_and(|modifiers| {
                                modifiers.iter().any(|modifier| {
                                    value_kind(modifier)
                                        == Some("redirectPlayerDamageToAttachedPermanent")
                                        && aura.controller == *player_id
                                })
                            }))
                        .then(|| aura.attached_to.clone())
                        .flatten()
                        .filter(|attached_id| self.permanent_position(attached_id).is_some())
                    })
                });
            if let Some(instance_id) = redirected_to {
                events.record_event(
                    "damageRedirected",
                    Some(player_id.clone()),
                    Some(instance_id.clone()),
                    json!({ "amount": amount, "source": source }),
                );
                TargetRef::Permanent { instance_id }
            } else {
                target
            }
        } else {
            target
        };
        (target, events.records)
    }
    pub(super) fn prepare_damage_to_redirected_target(
        &self,
        target: TargetRef,
        amount: i32,
        source: &str,
        combat: bool,
        source_snapshot: Option<&super::source_lki::ResolvedDamageSource>,
        initial_events: Vec<DeferredDamageEvent>,
    ) -> Result<PreparedDamage, EngineError> {
        let mut events = DamageEventCollector {
            records: initial_events,
        };
        let mut draws = Vec::new();
        if amount <= 0 {
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        let target_permanent_id = match &target {
            TargetRef::Permanent { instance_id } => Some(instance_id.as_str()),
            _ => None,
        };
        let damage_cannot_be_prevented =
            self.has_ecc_static_ability("damageCannotBePrevented", None);
        if !damage_cannot_be_prevented
            && combat
            && self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("preventCombatDamageByPermanent")
                    && modifier["expiresAfterTurn"].as_u64()
                        == Some(u64::from(self.state.turn_number))
                    && (modifier["permanentId"].as_str() == Some(source)
                        || target_permanent_id
                            .is_some_and(|id| modifier["permanentId"].as_str() == Some(id)))
            })
        {
            events.record_event(
                "damagePrevented",
                None,
                target_permanent_id.map(ToOwned::to_owned),
                json!({ "amount": amount, "combat": true, "source": source }),
            );
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        if !damage_cannot_be_prevented
            && self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("linkedDamagePrevention")
                    && modifier["damagingCardInstanceId"].as_str() == Some(source)
                    && modifier["sourceCardInstanceId"]
                        .as_str()
                        .and_then(|source_id| self.permanent_position(source_id))
                        .is_some()
            })
        {
            events.record_event(
                "damagePrevented",
                None,
                target_permanent_id.map(ToOwned::to_owned),
                json!({ "amount": amount, "source": source }),
            );
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        let source_card = if let Some(profile) = source_snapshot {
            if profile.card.instance_id != source {
                return Err(EngineError::new("damage source profile identity mismatch"));
            }
            Some(profile.card.clone())
        } else {
            self.state
                .players
                .iter()
                .flat_map(|player| player.battlefield.iter())
                .chain(self.state.stack.iter().map(|object| &object.card))
                .find(|card| card.instance_id == source)
                .cloned()
        };
        if !damage_cannot_be_prevented
            && let TargetRef::Permanent { instance_id } = &target
            && let Some(source_card) = &source_card
            && self
                .permanent_position(instance_id)
                .is_some_and(|(player_index, card_index)| {
                    let permanent = &self.state.players[player_index].battlefield[card_index];
                    self.permanent_is_protected_from_card(permanent, &source_card.definition)
                        || (permanent.controller != source_card.controller
                            && self.permanent_has_keyword(permanent, "protectionFromOpponents"))
                })
        {
            events.record_event(
                "damagePrevented",
                None,
                Some(instance_id.clone()),
                json!({ "amount": amount, "reason": "protection", "source": source }),
            );
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        let source_characteristics = if let Some(profile) = source_snapshot {
            let properties = &profile.properties;
            Some((
                properties.controller_id.clone(),
                properties.deathtouch,
                properties.lifelink,
                properties.wither
                    || properties.infect
                    || self.has_ecc_static_ability("allDamageHasWither", None),
            ))
        } else {
            source_card.as_ref().map(|card| {
                (
                    card.controller.clone(),
                    self.permanent_has_keyword(card, "deathtouch"),
                    self.permanent_has_keyword(card, "lifelink"),
                    self.permanent_has_keyword(card, "wither")
                        || self.permanent_has_keyword(card, "infect")
                        || self.has_ecc_static_ability("allDamageHasWither", None),
                )
            })
        };
        if !damage_cannot_be_prevented
            && combat
            && let TargetRef::Player { player_id } = &target
            && self.state.rule_modifiers.iter().any(|modifier| {
                value_kind(modifier) == Some("sokratesDialogue")
                    && modifier["cardInstanceId"].as_str() == Some(source)
                    && modifier["expiresAfterTurn"].as_u64()
                        == Some(u64::from(self.state.turn_number))
            })
        {
            let draw_count = amount.max(0) / 2;
            if draw_count > 0 {
                if let Some((source_controller, _, _, _)) = &source_characteristics {
                    draws.push((source_controller.clone(), draw_count as usize));
                }
                draws.push((player_id.clone(), draw_count as usize));
            }
            events.record_event(
                "damagePrevented",
                Some(player_id.clone()),
                Some(source.to_string()),
                json!({
                    "amount": amount,
                    "combat": true,
                    "reason": "sokratesDialogue",
                    "cardsDrawnEach": draw_count,
                }),
            );
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        let source_is_creature = source_snapshot
            .map(|profile| profile.properties.is_creature)
            .unwrap_or_else(|| {
                source_card
                    .as_ref()
                    .is_some_and(|card| self.permanent_is_creature(card))
            });
        let protected_player = match &target {
            TargetRef::Player { player_id } => Some(player_id.clone()),
            TargetRef::Permanent { instance_id } => {
                self.permanent_position(instance_id)
                    .map(|(player_index, card_index)| {
                        self.state.players[player_index].battlefield[card_index]
                            .controller
                            .clone()
                    })
            }
            _ => None,
        };
        if !damage_cannot_be_prevented
            && let TargetRef::Player { player_id } = &target
            && (self.player_has_battlefield_static_modifier(player_id, "preventPlayerDamage")
                || self.state.rule_modifiers.iter().any(|modifier| {
                    value_kind(modifier) == Some("protectionFromEverything")
                        && modifier["playerId"].as_str() == Some(player_id.as_str())
                }))
        {
            events.record_event(
                "damagePrevented",
                Some(player_id.clone()),
                Some(source.to_string()),
                json!({ "amount": amount, "reason": "preventAllPlayerDamage" }),
            );
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        if !damage_cannot_be_prevented
            && source_is_creature
            && source_characteristics
                .as_ref()
                .is_some_and(|(source_controller, _, _, _)| {
                    protected_player.as_ref().is_some_and(|player_id| {
                        source_controller != player_id
                            && self.state.rule_modifiers.iter().any(|modifier| {
                                value_kind(modifier) == Some("preventOpponentCreatureDamage")
                                    && modifier["playerId"].as_str() == Some(player_id.as_str())
                            })
                    })
                })
        {
            events.record_event(
                "damagePrevented",
                protected_player,
                Some(source.to_string()),
                json!({ "amount": amount, "reason": "opponentCreatureDamage" }),
            );
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        let mut damage_bonus = 0_i32;
        let mut damage_multiplier = 1_i32;
        if let Some((controller, _, _, _)) = &source_characteristics {
            if protected_player
                .as_ref()
                .is_some_and(|protected| protected != controller)
            {
                for (modifier_source, modifier) in self.battlefield_static_modifiers() {
                    if modifier_source.controller != *controller {
                        continue;
                    }
                    let bonus = match value_kind(&modifier) {
                        Some("increaseDamageBySourceCounter") => modifier["counter"]
                            .as_str()
                            .and_then(|name| modifier_source.counters.get(name))
                            .copied()
                            .unwrap_or(0),
                        Some("increaseControlledNoncombatDamage") if !combat => {
                            integer_value(&modifier["amount"]).ok_or_else(|| {
                                EngineError::new("damage bonus has an invalid integer")
                            })?
                        }
                        _ => continue,
                    };
                    damage_bonus = damage_bonus
                        .checked_add(bonus)
                        .ok_or_else(|| EngineError::new("damage bonus sum overflow"))?;
                }
            }
            for modifier in &self.state.rule_modifiers {
                if value_kind(modifier) == Some("damageMultiplier")
                    && modifier["playerId"].as_str() == Some(controller.as_str())
                {
                    let factor = integer_value(&modifier["factor"])
                        .filter(|factor| *factor >= 1)
                        .ok_or_else(|| {
                            EngineError::new("damage multiplier must be a positive integer")
                        })?;
                    damage_multiplier = damage_multiplier
                        .checked_mul(factor)
                        .ok_or_else(|| EngineError::new("damage multiplier product overflow"))?;
                }
            }
            if let Some(source_card) = &source_card {
                for (modifier_source, modifier) in self.battlefield_static_modifiers() {
                    if value_kind(&modifier) == Some("doubleDamageFromChosenType")
                        && modifier_source.controller == *controller
                        && Self::chosen_creature_type_matches(
                            &modifier_source,
                            &source_card.definition,
                            &json!({"decisionId":modifier["decisionId"]}),
                        )
                    {
                        damage_multiplier = damage_multiplier.checked_mul(2).ok_or_else(|| {
                            EngineError::new("chosen-type damage multiplier overflow")
                        })?;
                    }
                }
            }
        }
        let mut amount = amount
            .checked_add(damage_bonus)
            .and_then(|amount| amount.checked_mul(damage_multiplier))
            .ok_or_else(|| EngineError::new("modified damage amount overflow"))?;
        if !damage_cannot_be_prevented
            && let TargetRef::Player { player_id } = &target
            && source_characteristics
                .as_ref()
                .is_some_and(|(source_controller, _, _, _)| source_controller != player_id)
        {
            let prevented = self
                .battlefield_static_modifiers()
                .iter()
                .filter(|(modifier_source, modifier)| {
                    modifier_source.controller == *player_id
                        && value_kind(modifier) == Some("preventOpponentDamageAmount")
                })
                .filter_map(|(_, modifier)| integer_value(&modifier["amount"]))
                .try_fold(0_i32, |sum, value| sum.checked_add(value))
                .ok_or_else(|| EngineError::new("damage prevention sum overflow"))?
                .max(0);
            if prevented > 0 {
                let prevented = prevented.min(amount.max(0));
                amount = amount
                    .checked_sub(prevented)
                    .ok_or_else(|| EngineError::new("damage prevention subtraction overflow"))?;
                events.record_event(
                    "damagePrevented",
                    Some(player_id.clone()),
                    Some(source.to_string()),
                    json!({ "amount": prevented, "reason": "opponentSourceDamageReduction" }),
                );
            }
        }
        if amount <= 0 {
            return Ok(PreparedDamage::Prevented {
                events: events.records,
                draws,
                counter_removals: Vec::new(),
            });
        }
        let mut counter_removals = Vec::new();
        if let TargetRef::Permanent { instance_id } = &target {
            if let Some((p, c)) = self.permanent_position(instance_id) {
                let permanent = &self.state.players[p].battlefield[c];
                if source_is_creature
                    && self.permanent_has_self_static_modifier(
                        permanent,
                        "preventCreatureDamageToSelf",
                    )
                {
                    events.record_event(
                        "damagePrevented",
                        None,
                        Some(instance_id.clone()),
                        json!({"amount":amount,"source":source,"reason":"creatureSource"}),
                    );
                    return Ok(PreparedDamage::Prevented {
                        events: events.records,
                        draws,
                        counter_removals,
                    });
                }
                if self.static_modifier_applies_to("preventDamage", permanent) {
                    events.record_event(
                        "damagePrevented",
                        None,
                        Some(instance_id.clone()),
                        json!({"amount":amount,"source":source}),
                    );
                    return Ok(PreparedDamage::Prevented {
                        events: events.records,
                        draws,
                        counter_removals,
                    });
                }
                if let Some(removal) =
                    self.prepare_counter_removal(instance_id, "shield", 1, "wouldBeDealtDamage")
                {
                    events.record_event(
                        "counterRemoved",
                        Some(removal.controller.clone()),
                        Some(instance_id.clone()),
                        removal.event_detail(),
                    );
                    counter_removals.push(removal);
                    if !damage_cannot_be_prevented {
                        events.record_event(
                            "damagePrevented",
                            None,
                            Some(instance_id.clone()),
                            json!({"amount":amount,"source":source,"reason":"shieldCounter"}),
                        );
                        return Ok(PreparedDamage::Prevented {
                            events: events.records,
                            draws,
                            counter_removals,
                        });
                    }
                }
            }
        }
        Ok(PreparedDamage::Ready(PreparedDamageFrame {
            target,
            amount,
            source_characteristics,
            events: events.records,
            counter_removals,
        }))
    }
    pub(super) fn publish_prepared_damage_events(
        &mut self,
        events: Vec<DeferredDamageEvent>,
        batch_id: Option<usize>,
    ) {
        for mut event in events {
            if let Some(batch_id) = batch_id {
                event.detail["batchId"] = json!(batch_id);
            }
            self.record_event(event.kind, event.player_id, event.card_id, event.detail);
        }
    }
    pub(super) fn commit_prevented_damage(
        &mut self,
        events: Vec<DeferredDamageEvent>,
        draws: Vec<(String, usize)>,
        counter_removals: Vec<PreparedCounterRemoval>,
        batch_id: Option<usize>,
    ) -> Result<(), EngineError> {
        for removal in &counter_removals {
            if !self.commit_prepared_counter_removal(removal) {
                return Err(EngineError::new("prepared shield counter context changed"));
            }
        }
        for (player, count) in draws {
            self.draw_cards_without_state_check(&player, count)?;
        }
        self.publish_prepared_damage_events(events, batch_id);
        Ok(())
    }
}
