//! Stable planning for a collection of permanents destroyed by one instruction.
//!
//! A collection is selected before any member is destroyed. Protections,
//! replacement effects, and trigger sources that apply to that destruction must
//! therefore be captured from the same pre-destruction battlefield. Re-reading
//! the battlefield after every removal makes the outcome depend on iteration
//! order.

pub(super) use super::battlefield_departure::FrozenGraveyardDestination;
use super::battlefield_departure::{
    GraveyardMoveCause, PlannedGraveyardDeparture, exactly_one_or_none,
};
use super::*;

#[derive(Clone)]
enum PlannedDestructionOutcome {
    UmbraArmor { aura_id: String },
    Regenerated,
    Indestructible,
    ShieldCounter,
    ZoneChange(PlannedGraveyardDeparture),
}

impl PlannedDestructionOutcome {
    fn counts_as_destroyed(&self) -> bool {
        matches!(self, Self::ZoneChange(_))
    }
}

struct PlannedDestruction {
    permanent: CardInstance,
    remove_regeneration_shield: bool,
    outcome: PlannedDestructionOutcome,
}

struct BatchPublication {
    permanent: CardInstance,
    died: Option<CardInstance>,
    destroyed: bool,
    on_replaced: Option<(CardInstance, Value)>,
}

fn validate_destruction_replacement_choice(
    instance_id: &str,
    has_umbra_armor: bool,
    has_regeneration_shield: bool,
    has_shield_counter: bool,
) -> Result<(), EngineError> {
    let applicable = usize::from(has_umbra_armor)
        + usize::from(has_regeneration_shield)
        + usize::from(has_shield_counter);
    if applicable > 1 {
        return Err(EngineError::new(format!(
            "destruction batch has multiple applicable destruction replacements for {instance_id}"
        )));
    }
    Ok(())
}

impl GameEngine {
    fn plan_collection_destruction(
        &self,
        instance_ids: Vec<String>,
        cannot_regenerate: bool,
    ) -> Result<Vec<PlannedDestruction>, EngineError> {
        let mut seen_instance_ids = BTreeSet::new();
        let instance_ids = instance_ids
            .into_iter()
            .filter(|instance_id| seen_instance_ids.insert(instance_id.clone()))
            .collect::<Vec<_>>();
        let candidate_ids = instance_ids.iter().cloned().collect::<BTreeSet<_>>();
        let static_modifiers = self.battlefield_static_modifiers();
        let mut planned = Vec::new();

        for instance_id in instance_ids {
            let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                continue;
            };
            let permanent = self.state.players[player_index].battlefield[card_index].clone();

            if self.permanent_has_keyword(&permanent, "indestructible") {
                planned.push(PlannedDestruction {
                    permanent,
                    remove_regeneration_shield: false,
                    outcome: PlannedDestructionOutcome::Indestructible,
                });
                continue;
            }

            let umbra_armor = exactly_one_or_none(
                self.state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|attachment| {
                        attachment.attached_to.as_deref() == Some(instance_id.as_str())
                            && self.permanent_has_keyword(attachment, "umbraArmor")
                    })
                    .map(|attachment| attachment.instance_id.clone())
                    .collect::<Vec<_>>(),
                || {
                    EngineError::new(format!(
                        "destruction batch has multiple Umbra Armor replacements for {instance_id}"
                    ))
                },
            )?;
            let has_regeneration_shield = !cannot_regenerate
                && self.state.rule_modifiers.iter().any(|modifier| {
                    value_kind(modifier) == Some("regenerationShield")
                        && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str())
                });
            let has_shield_counter = permanent.counters.get("shield").copied().unwrap_or(0) > 0;
            validate_destruction_replacement_choice(
                &instance_id,
                umbra_armor.is_some(),
                has_regeneration_shield,
                has_shield_counter,
            )?;

            if let Some(aura_id) = umbra_armor {
                if candidate_ids.contains(&aura_id) {
                    return Err(EngineError::new(format!(
                        "destruction batch cannot yet order Umbra Armor {aura_id} when it is also selected"
                    )));
                }
                planned.push(PlannedDestruction {
                    permanent,
                    remove_regeneration_shield: false,
                    outcome: PlannedDestructionOutcome::UmbraArmor { aura_id },
                });
                continue;
            }

            if has_regeneration_shield {
                planned.push(PlannedDestruction {
                    permanent,
                    remove_regeneration_shield: false,
                    outcome: PlannedDestructionOutcome::Regenerated,
                });
                continue;
            }

            if has_shield_counter {
                planned.push(PlannedDestruction {
                    permanent,
                    remove_regeneration_shield: false,
                    outcome: PlannedDestructionOutcome::ShieldCounter,
                });
                continue;
            }

            let zone_change = self.plan_graveyard_departure(
                &permanent,
                &static_modifiers,
                GraveyardMoveCause::Destruction,
            )?;
            planned.push(PlannedDestruction {
                permanent,
                remove_regeneration_shield: cannot_regenerate,
                outcome: PlannedDestructionOutcome::ZoneChange(zone_change),
            });
        }

        Ok(planned)
    }

    pub(super) fn destroy_permanent_collection_from_stack(
        &mut self,
        instance_ids: Vec<String>,
        cannot_regenerate: bool,
        stack_object: &StackObject,
    ) -> Result<Vec<String>, EngineError> {
        if instance_ids.is_empty() {
            return Ok(Vec::new());
        }
        let planned = self.plan_collection_destruction(instance_ids, cannot_regenerate)?;
        if planned.is_empty() {
            return Ok(Vec::new());
        }
        let death_trigger_sources = self.capture_death_trigger_source_snapshot();
        let destruction_trigger_sources = death_trigger_sources.sources();
        let mut destroyed_ids = Vec::new();
        let mut publications = Vec::new();

        // Freeze every actual departing source while all continuous grants
        // still exist. Survivors of regeneration/shield/indestructibility are
        // not marked departed. Nested Umbra sacrifice captures its real source.
        for entry in &planned {
            if matches!(&entry.outcome, PlannedDestructionOutcome::ZoneChange(_)) {
                self.capture_pending_source_lki(&entry.permanent.instance_id);
            }
        }
        for planned_destruction in planned {
            let instance_id = planned_destruction.permanent.instance_id.clone();
            if self.permanent_position(&instance_id).is_none() {
                continue;
            }
            if planned_destruction.remove_regeneration_shield {
                self.state.rule_modifiers.retain(|modifier| {
                    !(value_kind(modifier) == Some("regenerationShield")
                        && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str()))
                });
            }

            let mut died = None;
            let mut on_replaced_publication = None;
            match &planned_destruction.outcome {
                PlannedDestructionOutcome::UmbraArmor { aura_id } => {
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    {
                        self.state.players[player_index].battlefield[card_index].damage_marked = 0;
                    }
                    self.move_permanent_to_graveyard(aura_id, "destroyed")?;
                    self.record_event(
                        "destructionReplacedByUmbraArmor",
                        None,
                        Some(instance_id.clone()),
                        json!({ "auraInstanceId": aura_id }),
                    );
                }
                PlannedDestructionOutcome::Regenerated => {
                    self.consume_regeneration_shield(&instance_id, "destroyed");
                }
                PlannedDestructionOutcome::Indestructible => {}
                PlannedDestructionOutcome::ShieldCounter => {
                    self.remove_counters_from_permanent(
                        &instance_id,
                        "shield",
                        1,
                        "wouldBeDestroyed",
                    );
                    self.record_event(
                        "destructionPrevented",
                        None,
                        Some(instance_id.clone()),
                        json!({ "reason": "shieldCounter" }),
                    );
                }
                PlannedDestructionOutcome::ZoneChange(plan) => {
                    let installed = self.install_planned_graveyard_departure(
                        &planned_destruction.permanent,
                        plan,
                        "destroyed",
                    )?;
                    self.publish_installed_graveyard_departure_with_sources(
                        &installed,
                        "destroyed",
                        Some(death_trigger_sources.sources()),
                    )?;
                    died = installed.died();
                    on_replaced_publication = installed.on_replaced;
                }
            }

            let destroyed = planned_destruction.outcome.counts_as_destroyed()
                && self.permanent_position(&instance_id).is_none();
            if destroyed {
                destroyed_ids.push(instance_id);
            }
            publications.push(BatchPublication {
                permanent: planned_destruction.permanent,
                died,
                destroyed,
                on_replaced: on_replaced_publication,
            });
        }

        for publication in publications {
            if let Some((source, on_replaced)) = publication.on_replaced {
                self.put_triggered_ability_on_stack_with_origin(
                    &source,
                    on_replaced,
                    BTreeMap::new(),
                    trigger_multiplicity::TriggerOrigin::Unproven,
                );
                self.check_quantity_failure()?;
            }
            if let Some(died) = publication.died.as_ref() {
                self.publish_committed_death_triggers(died, "destroyed", &death_trigger_sources);
            }
            if !publication.destroyed {
                continue;
            }
            self.record_event(
                "permanentDestroyed",
                Some(stack_object.controller.clone()),
                Some(publication.permanent.instance_id.clone()),
                json!({
                    "destroyedControllerId": publication.permanent.controller,
                }),
            );
            if publication.permanent.controller == stack_object.controller {
                continue;
            }
            for trigger_source in destruction_trigger_sources {
                if trigger_source.source.controller != publication.permanent.controller {
                    continue;
                }
                for rule in trigger_source.rules.iter().filter(|rule| {
                    value_kind(&rule["event"]) == Some("opponentDestroyedControlledPermanent")
                }) {
                    if !matches_card_filter(
                        &publication.permanent.definition,
                        &rule["event"]["where"],
                    ) {
                        continue;
                    }
                    self.put_triggered_ability_on_stack_with_multiplicity(
                        &trigger_source.source,
                        rule.clone(),
                        BTreeMap::from([(
                            "triggeringPlayerId".to_string(),
                            Value::String(stack_object.controller.clone()),
                        )]),
                        Some(&trigger_source.multiplicity),
                        trigger_multiplicity::CapturedOccurrence::Destruction,
                    );
                    self.check_quantity_failure()?;
                }
            }
        }

        Ok(destroyed_ids)
    }

    pub(super) fn publish_committed_death_triggers(
        &mut self,
        died: &CardInstance,
        reason: &str,
        trigger_sources: &death_trigger_batch::DeathTriggerSourceSnapshot,
    ) {
        self.enqueue_permanent_died_triggers(died, reason, trigger_sources);
        if let Some(modifier) = self
            .state
            .rule_modifiers
            .iter()
            .find(|modifier| {
                value_kind(modifier) == Some("drawIfDiesThisTurn")
                    && modifier["instanceId"].as_str() == Some(died.instance_id.as_str())
                    && modifier["expiresAfterTurn"].as_u64()
                        == Some(u64::from(self.state.turn_number))
            })
            .cloned()
        {
            let mut source = died.clone();
            if let Some(player_id) = modifier["playerId"].as_str() {
                source.controller = player_id.to_string();
            }
            self.put_triggered_ability_on_stack_with_origin(
                &source,
                json!({
                    "kind": "triggeredAbility",
                    "source": { "kind": "self" },
                    "effects": [{
                        "kind": "drawCards",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "count": { "kind": "integer", "value": 1 },
                    }],
                }),
                BTreeMap::new(),
                trigger_multiplicity::TriggerOrigin::Unproven,
            );
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "destruction_batch_tests.rs"]
mod tests;
