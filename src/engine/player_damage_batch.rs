//! One source's simultaneous noncombat damage to a typed player set.
//! Shared damage preparation owns replacement/prevention; this module owns
//! collection, checked consequence plans, one commit, and grouped publication.
use super::counter_placement::PreparedCounterRemoval;
use super::damage_preparation::{DeferredDamageEvent, PreparedDamage};
use super::life::PreparedLifeGain;
use super::*;

#[derive(Clone)]
pub(super) struct PlayerDamageRecord {
    pub player_id: String,
    pub amount: i32,
    pub infect: bool,
}
struct PlayerStatePlan {
    id: String,
    life_before: i32,
    life_after: i32,
    poison_before: i32,
    poison_after: i32,
}
pub(super) struct PreparedPlayerDamageBatch {
    event_sequence: usize,
    source: String,
    controller: String,
    states: Vec<PlayerStatePlan>,
    records: Vec<PlayerDamageRecord>,
    events: Vec<DeferredDamageEvent>,
    removals: Vec<PreparedCounterRemoval>,
    life_gain: Option<PreparedLifeGain>,
}

impl GameEngine {
    /// This boundary deliberately excludes combat, unprevented redirection to
    /// permanents and untyped legacy aggregate observers. It fails before any
    /// damage, counter, event or trigger mutation, rather than approximating.
    pub(super) fn prepare_player_set_damage(
        &self,
        recipients: &[String],
        amount: i32,
        source: &str,
        source_profile: Option<&super::source_lki::ResolvedDamageSource>,
    ) -> Result<PreparedPlayerDamageBatch, EngineError> {
        let source_card = if let Some(profile) = source_profile {
            if profile.card.instance_id != source {
                return Err(EngineError::new(
                    "damage batch source profile identity mismatch",
                ));
            }
            &profile.card
        } else {
            self.state
                .players
                .iter()
                .flat_map(|player| player.battlefield.iter())
                .chain(self.state.stack.iter().map(|object| &object.card))
                .find(|card| card.instance_id == source)
                .ok_or_else(|| EngineError::new("damage batch requires a resolved source"))?
        };
        if source_card.flags.get("phasedOut").copied().unwrap_or(false) {
            return Err(EngineError::new(
                "phased-out damage source requires effective source LKI",
            ));
        }
        let source_controller = source_profile
            .map(|profile| profile.properties.controller_id.clone())
            .unwrap_or_else(|| source_card.controller.clone());
        self.require_player_damage_batch_observers()?;
        let mut batch = PreparedPlayerDamageBatch {
            event_sequence: self.state.events.len(),
            source: source.into(),
            controller: source_controller.clone(),
            states: Vec::new(),
            records: Vec::new(),
            events: Vec::new(),
            removals: Vec::new(),
            life_gain: None,
        };
        if amount <= 0 {
            return Ok(batch);
        }
        // Reject redirections before evaluating or committing damage. Their
        // order relative to amount/prevention replacements needs a real CR616
        // affected-player choice protocol, not forced coalescence.
        let mut unique_players = BTreeSet::new();
        let mut direct_recipients: Vec<(TargetRef, i32, Vec<DeferredDamageEvent>)> = Vec::new();
        for player in recipients {
            if !unique_players.insert(player.clone()) {
                return Err(EngineError::new("duplicate damage player"));
            }
            let p = self.player_index(player)?;
            if self.state.players[p].has_lost {
                return Err(EngineError::new("damage player has left the game"));
            }
            let (target, events) = self.prepare_damage_redirection(
                TargetRef::Player {
                    player_id: player.clone(),
                },
                amount,
                source,
            );
            if target
                != (TargetRef::Player {
                    player_id: player.clone(),
                })
            {
                return Err(EngineError::new(
                    "simultaneous damage redirection requires ordered replacement choices",
                ));
            }
            direct_recipients.push((target, amount, events));
        }

        let mut total = 0_i32;
        for (target, amount, events) in direct_recipients {
            let TargetRef::Player { player_id } = &target else {
                return Err(EngineError::new(
                    "player damage plan contains a nonplayer recipient",
                ));
            };
            let is_creature = source_profile
                .map(|profile| profile.properties.is_creature)
                .unwrap_or_else(|| self.permanent_is_creature(source_card));
            self.require_player_damage_replacement_order(
                player_id,
                source,
                &source_controller,
                source_card,
                is_creature,
            )?;
            match self.prepare_damage_to_redirected_target(
                target,
                amount,
                source,
                false,
                source_profile,
                events,
                &mut super::quantity_replacement::OptionalQuantityProvider(None),
            )? {
                PreparedDamage::Prevented {
                    events,
                    draws,
                    counter_removals,
                } => {
                    if !draws.is_empty() {
                        return Err(EngineError::new(
                            "damage batch replacement draws are unsupported",
                        ));
                    }
                    batch.events.extend(events);
                    batch.removals.extend(counter_removals);
                }
                PreparedDamage::Ready(frame) => {
                    let TargetRef::Player { player_id } = frame.target else {
                        return Err(EngineError::new(
                            "unprevented redirected permanent damage requires a typed permanent consequence batch",
                        ));
                    };
                    let p = self.player_index(&player_id)?;
                    let player = &self.state.players[p];
                    let infect = source_profile
                        .map(|profile| profile.properties.infect)
                        .unwrap_or_else(|| self.permanent_has_keyword(source_card, "infect"));
                    let life_locked = self.state.rule_modifiers.iter().any(|m| {
                        value_kind(m) == Some("lifeTotalCannotChange")
                            && m["playerId"].as_str() == Some(player_id.as_str())
                    });
                    let poison_before = player.counters.get("poison").copied().unwrap_or(0);
                    let life_after = if infect || life_locked {
                        player.life
                    } else {
                        player
                            .life
                            .checked_sub(frame.amount)
                            .ok_or_else(|| EngineError::new("damage life overflow"))?
                    };
                    let poison_after = if infect {
                        poison_before
                            .checked_add(frame.amount)
                            .ok_or_else(|| EngineError::new("damage poison overflow"))?
                    } else {
                        poison_before
                    };
                    batch.events.extend(frame.events);
                    batch.removals.extend(frame.counter_removals);
                    if life_locked && !infect {
                        batch.events.push(DeferredDamageEvent {kind:"lifeChangePrevented",player_id:Some(player_id.clone()),card_id:Some(source.into()),detail:json!({"amount":-frame.amount,"reason":"lifeTotalCannotChange"}),shield:None});
                    }
                    batch.states.push(PlayerStatePlan {
                        id: player_id.clone(),
                        life_before: player.life,
                        life_after,
                        poison_before,
                        poison_after,
                    });
                    batch.records.push(PlayerDamageRecord {
                        player_id,
                        amount: frame.amount,
                        infect,
                    });
                    total = total
                        .checked_add(frame.amount)
                        .ok_or_else(|| EngineError::new("damage total overflow"))?;
                }
            }
        }
        // Each recipient is prepared against the same shield state; a finite
        // shield consumed by several recipients needs an ordered choice.
        let mut finite_shields = BTreeSet::new();
        for event in &batch.events {
            if let Some((shield_id, _)) = event.shield
                && !finite_shields.insert(shield_id)
            {
                return Err(EngineError::new(
                    "one finite prevention shield applying to several simultaneous recipients requires ordered replacement choices",
                ));
            }
        }
        if total > 0
            && source_profile
                .map(|profile| profile.properties.lifelink)
                .unwrap_or_else(|| self.permanent_has_keyword(source_card, "lifelink"))
        {
            let base_life = if let Some(state) = batch
                .states
                .iter()
                .find(|state| state.id == source_controller)
            {
                state.life_after
            } else {
                self.state.players[self.player_index(&source_controller)?].life
            };
            let gain = self.prepare_life_gain_at_life(&source_controller, total, base_life)?;
            if let Some(gain) = &gain {
                if let Some(state) = batch
                    .states
                    .iter_mut()
                    .find(|state| state.id == source_controller)
                {
                    state.life_after = gain.life_after;
                } else {
                    let p = self.player_index(&source_controller)?;
                    let poison = self.state.players[p]
                        .counters
                        .get("poison")
                        .copied()
                        .unwrap_or(0);
                    batch.states.push(PlayerStatePlan {
                        id: source_controller.clone(),
                        life_before: gain.life_before,
                        life_after: gain.life_after,
                        poison_before: poison,
                        poison_after: poison,
                    });
                }
            }
            batch.life_gain = gain;
        }
        Ok(batch)
    }

    pub(super) fn commit_player_set_damage(
        &mut self,
        batch: PreparedPlayerDamageBatch,
    ) -> Result<(), EngineError> {
        if self.state.events.len() != batch.event_sequence {
            return Err(EngineError::new("damage batch event context changed"));
        }
        // Validate all plans before mutating any state. Between preparation and
        // commit there are no decision callbacks, events or replacement choices.
        for state in &batch.states {
            let p = self.player_index(&state.id)?;
            let player = &self.state.players[p];
            if player.has_lost
                || player.life != state.life_before
                || player.counters.get("poison").copied().unwrap_or(0) != state.poison_before
            {
                return Err(EngineError::new("damage batch player context changed"));
            }
        }
        for removal in &batch.removals {
            if !self.counter_removal_context_matches(removal) {
                return Err(EngineError::new("damage batch counter context changed"));
            }
        }
        for state in &batch.states {
            let p = self.player_index(&state.id)?;
            self.state.players[p].life = state.life_after;
            if state.poison_after == 0 {
                self.state.players[p].counters.remove("poison");
            } else {
                self.state.players[p]
                    .counters
                    .insert("poison".into(), state.poison_after);
            }
        }
        for removal in &batch.removals {
            let committed = self.commit_prepared_counter_removal(removal);
            debug_assert!(committed);
        }
        // Reserve the identity from the next real event sequence. No allocator,
        // copy counter or unrelated engine clock is involved.
        let batch_id = self.state.events.len() + 1;
        self.publish_prepared_damage_events(batch.events, Some(batch_id));
        if !batch.records.is_empty() {
            self.record_event("damageDealtTogether",Some(batch.controller.clone()),Some(batch.source.clone()),json!({"batchId":batch_id,"combat":false,"source":batch.source,"recipients":batch.records.iter().map(|r|json!({"playerId":r.player_id,"amount":r.amount})).collect::<Vec<_>>()}));
            for r in &batch.records {
                self.record_event("damageDealt",Some(r.player_id.clone()),None,json!({"batchId":batch_id,"amount":r.amount,"combat":false,"source":batch.source,"sourceControllerId":batch.controller,"infect":r.infect,"toxic":0}));
            }
            if let Some(gain) = &batch.life_gain {
                self.record_prepared_life_gain(gain);
            }
            // Every observer sees all consequences, including simultaneous
            // lifelink, instead of observing an intermediate player's damage.
            self.enqueue_player_damage_observers(
                &batch.controller,
                &batch.source,
                false,
                &batch.records,
                Some(batch_id),
            );
            for r in &batch.records {
                self.enqueue_player_damage_extra_observers(
                    &batch.controller,
                    &r.player_id,
                    &batch.source,
                );
                // One dealt-damage event per damaged player (CR 120.1).
                self.enqueue_source_damage_observers(
                    &batch.source,
                    Some(batch.controller.as_str()),
                    super::source_damage_observation::DamageRecipientRef::Player(&r.player_id),
                    r.amount,
                    false,
                );
            }
        }
        if let Some(gain) = batch.life_gain {
            self.notify_prepared_life_gain(&gain);
        }
        Ok(())
    }

    pub(super) fn execute_player_set_damage(
        &mut self,
        effect: &Value,
        stack: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &BTreeMap<String, Value>,
    ) -> Result<bool, EngineError> {
        if value_kind(effect) != Some("dealDamage")
            || !matches!(
                value_kind(&effect["recipient"]),
                Some("opponentsOf" | "eachPlayer")
            )
        {
            return Ok(false);
        }
        let amount = self
            .runtime_numeric_expression(&effect["amount"], stack, bindings, decisions)
            .ok_or_else(|| EngineError::new("damage batch amount has no numeric producer"))?;
        if amount <= 0 {
            return Ok(true);
        }
        if effect.get("bindExcessAs").is_some() || effect.get("bindRecipientAs").is_some() {
            return Err(EngineError::new(
                "plural damage does not define a singular excess or recipient binding",
            ));
        }
        let source = match effect.get("source") {
            None => stack.card.instance_id.clone(),
            Some(reference) if matches!(value_kind(reference), Some("self" | "abilitySource")) => {
                stack.card.instance_id.clone()
            }
            Some(reference) => match self.resolve_target(reference, stack, bindings) {
                Some(TargetRef::Permanent { instance_id } | TargetRef::Card { instance_id }) => {
                    instance_id
                }
                _ => {
                    return Err(EngineError::new(
                        "damage batch source reference is unresolved or not an object",
                    ));
                }
            },
        };
        let players = self.player_iteration_ids(&effect["recipient"], stack, bindings)?;
        let profile = if source == stack.card.instance_id {
            Some(self.resolve_damage_source_lki(stack)?)
        } else {
            None
        };
        let batch = self.prepare_player_set_damage(&players, amount, &source, profile.as_ref())?;
        self.commit_player_set_damage(batch)?;
        Ok(true)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "player_damage_batch_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "player_damage_source_reference_tests.rs"]
mod source_reference_tests;
