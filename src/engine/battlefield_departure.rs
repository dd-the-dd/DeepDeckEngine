//! Shared typed destination plans and physical/publication departure phases.
//! Destruction prevention belongs to destruction_batch, never to this domain.
use super::card_zone_transition::{
    ObjectTransitionReason, ObjectZone, OwnedMovementCard, reset_movement_card,
};
use super::replacement_pipeline::{GraveyardReplacementOrigin, ReplacementOrder};
use super::*;

#[derive(Clone, Copy)]
pub(super) enum GraveyardMoveCause {
    Destruction,
    Sacrifice,
}
impl GraveyardMoveCause {
    fn diagnostic_prefix(self) -> &'static str {
        match self {
            Self::Destruction => "destruction batch",
            Self::Sacrifice => "sacrifice batch",
        }
    }
}
#[derive(Clone)]
pub(super) enum FrozenGraveyardDestination {
    EvaluateLive,
    Graveyard,
    Exile {
        replacement_source_id: String,
        replacement_controller_id: Option<String>,
    },
}
#[derive(Clone)]
pub(super) enum PlannedGraveyardOutcome {
    DeathReplacedWithExile {
        source: CardInstance,
        on_replaced: Option<Value>,
    },
    LeavingBattlefieldReplacedWithExile,
    DeathReplacedByRuleModifier {
        modifier_kind: String,
    },
    MoveTo(FrozenGraveyardDestination),
}
#[derive(Clone)]
pub(super) struct PlannedGraveyardDeparture {
    pub(super) outcome: PlannedGraveyardOutcome,
    pub(super) consume_specific_graveyard_replacement: bool,
    pub(super) target_numeric_snapshot: Option<super::target_numeric_lki::PreparedTargetDeparture>,
}
#[derive(Clone, Copy)]
enum DepartureEvent {
    GraveyardMove,
    Exile,
}
pub(super) struct InstalledBattlefieldDeparture {
    pub(super) permanent: CardInstance,
    pub(super) destination: &'static str,
    was_earthbent: bool,
    event: DepartureEvent,
    observers: Vec<DepartureTriggerSource>,
}
impl InstalledBattlefieldDeparture {
    pub(super) fn died(&self) -> Option<CardInstance> {
        (self.destination == "graveyard").then(|| self.permanent.clone())
    }
}
#[derive(Clone, Copy)]
enum AfterDepartureEvent {
    LeavingReplacement,
    DeathRuleReplacement,
}
pub(super) struct InstalledGraveyardDeparture {
    pub(super) movement: Option<InstalledBattlefieldDeparture>,
    pub(super) on_replaced: Option<(CardInstance, Value)>,
    after_event: Option<AfterDepartureEvent>,
}
impl InstalledGraveyardDeparture {
    pub(super) fn died(&self) -> Option<CardInstance> {
        self.movement
            .as_ref()
            .and_then(InstalledBattlefieldDeparture::died)
    }
}
pub(super) fn exactly_one_or_none<T>(
    mut values: Vec<T>,
    ambiguity: impl FnOnce() -> EngineError,
) -> Result<Option<T>, EngineError> {
    if values.len() > 1 {
        return Err(ambiguity());
    }
    Ok(values.pop())
}
pub(super) fn validate_zone_or_death_replacement_choice(
    instance_id: &str,
    has_death_replacement: bool,
    has_leaving_replacement: bool,
    has_one_shot_death_replacement: bool,
    has_graveyard_replacement: bool,
    cause: GraveyardMoveCause,
) -> Result<(), EngineError> {
    let context = cause.diagnostic_prefix();
    let applicable = usize::from(has_death_replacement)
        + usize::from(has_leaving_replacement)
        + usize::from(has_one_shot_death_replacement)
        + usize::from(has_graveyard_replacement);
    if applicable > 1 {
        return Err(EngineError::new(format!(
            "{context} has multiple applicable zone or death replacements for {instance_id}"
        )));
    }
    Ok(())
}

pub(super) use super::trigger_multiplicity::CapturedTriggerSource as DepartureTriggerSource;
impl GameEngine {
    pub(super) fn capture_battlefield_departure_trigger_sources(
        &self,
    ) -> Vec<DepartureTriggerSource> {
        let modifiers = self.battlefield_static_modifiers();
        self.state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| &player.battlefield)
            .filter(|source| !source.flags.get("phasedOut").copied().unwrap_or(false))
            .map(|source| {
                // Effective layer (CR 613.1f): granted triggered abilities are
                // captured with the printed ones, before the departure.
                let rules = self
                    .effective_rules_with_modifiers(source, &modifiers)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.battlefield_observer_rule_active(source, rule)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                DepartureTriggerSource {
                    source: source.clone(),
                    rules,
                    multiplicity: self
                        .capture_trigger_multiplicity_from_modifiers(source, &modifiers),
                }
            })
            .collect()
    }

    pub(super) fn plan_graveyard_departure(
        &self,
        permanent: &CardInstance,
        static_modifiers: &[(CardInstance, Value)],
        cause: GraveyardMoveCause,
    ) -> Result<PlannedGraveyardDeparture, EngineError> {
        let instance_id = permanent.instance_id.clone();
        let context = cause.diagnostic_prefix();
        let death_replacement = exactly_one_or_none(
            static_modifiers
                .iter()
                .filter(|(source, modifier)| {
                    value_kind(modifier) == Some("exileOpponentCreatureInsteadOfDying")
                        && source.controller != permanent.controller
                        && modifier.get("where").map_or_else(
                            || self.permanent_is_creature(&permanent),
                            |where_filter| {
                                self.permanent_matches_effective_filter(&permanent, where_filter)
                            },
                        )
                })
                .map(|(source, modifier)| (source.clone(), modifier.get("onReplaced").cloned()))
                .collect::<Vec<_>>(),
            || {
                EngineError::new(format!(
                    "{context} has multiple death replacements for {instance_id}"
                ))
            },
        )?;
        let leaving_replacement = self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("exileIfLeavesBattlefield")
                && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str())
        });

        let death_rule_modifier = exactly_one_or_none(
            self.state
                .rule_modifiers
                .iter()
                .filter_map(|modifier| match value_kind(modifier) {
                    Some("exileIfWouldDie")
                        if modifier["cardInstanceId"].as_str() == Some(instance_id.as_str()) =>
                    {
                        Some("exileIfWouldDie".to_string())
                    }
                    Some("exileIfDiesThisTurn")
                        if modifier["instanceId"].as_str() == Some(instance_id.as_str())
                            && modifier["expiresAfterTurn"].as_u64()
                                == Some(u64::from(self.state.turn_number)) =>
                    {
                        Some("exileIfDiesThisTurn".to_string())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            || {
                EngineError::new(format!(
                    "{context} has multiple one-shot death replacements for {instance_id}"
                ))
            },
        )?;
        let owner_id = permanent.owner.clone();
        // Propose -> match -> order -> apply (replacement_pipeline.rs): one
        // graveyard-destination replacement at most may apply here.
        let graveyard_replacement = self
            .select_graveyard_destination_replacement(
                permanent,
                &owner_id,
                static_modifiers,
                &ReplacementOrder::RequireUnique {
                    ambiguity: format!(
                        "{context} has multiple graveyard destination replacements for {instance_id}"
                    ),
                },
            )?
            .map(|proposal| {
                let consume_specific = matches!(
                    proposal.program.origin,
                    GraveyardReplacementOrigin::SpecificCard { .. }
                );
                (
                    proposal.source_id,
                    proposal.program.controller,
                    consume_specific,
                )
            });
        validate_zone_or_death_replacement_choice(
            &instance_id,
            death_replacement.is_some(),
            leaving_replacement,
            death_rule_modifier.is_some(),
            graveyard_replacement.is_some(),
            cause,
        )?;
        let (outcome, consume_specific_graveyard_replacement) = if let Some((source, on_replaced)) =
            death_replacement
        {
            (
                PlannedGraveyardOutcome::DeathReplacedWithExile {
                    source,
                    on_replaced,
                },
                false,
            )
        } else if leaving_replacement {
            (
                PlannedGraveyardOutcome::LeavingBattlefieldReplacedWithExile,
                false,
            )
        } else if let Some(modifier_kind) = death_rule_modifier {
            (
                PlannedGraveyardOutcome::DeathReplacedByRuleModifier { modifier_kind },
                false,
            )
        } else if let Some((source_id, source_controller, consume_specific)) = graveyard_replacement
        {
            (
                PlannedGraveyardOutcome::MoveTo(FrozenGraveyardDestination::Exile {
                    replacement_source_id: source_id,
                    replacement_controller_id: source_controller,
                }),
                consume_specific,
            )
        } else {
            (
                PlannedGraveyardOutcome::MoveTo(FrozenGraveyardDestination::Graveyard),
                false,
            )
        };
        Ok(PlannedGraveyardDeparture {
            outcome,
            consume_specific_graveyard_replacement,
            target_numeric_snapshot: self.prepare_target_numeric_departure(&permanent.instance_id),
        })
    }
    pub(super) fn install_permanent_graveyard_move(
        &mut self,
        instance_id: &str,
        destination: FrozenGraveyardDestination,
    ) -> Result<Option<InstalledBattlefieldDeparture>, EngineError> {
        self.install_permanent_graveyard_move_with_target_numeric(instance_id, destination, None)
    }

    pub(super) fn install_permanent_graveyard_move_with_target_numeric(
        &mut self,
        instance_id: &str,
        destination: FrozenGraveyardDestination,
        prepared_target_numeric: Option<super::target_numeric_lki::PreparedTargetDeparture>,
    ) -> Result<Option<InstalledBattlefieldDeparture>, EngineError> {
        if self.permanent_position(instance_id).is_none() {
            return Ok(None);
        }
        let movement_source = self
            .prepare_card_movement_source(instance_id, None)?
            .ok_or_else(|| EngineError::new("battlefield departure source disappeared"))?;
        // Freeze replacement policy while the outgoing source is still live.
        let live_destination = if matches!(&destination, FrozenGraveyardDestination::EvaluateLive) {
            let (player, index) = self
                .permanent_position(instance_id)
                .ok_or_else(|| EngineError::new("graveyard policy source disappeared"))?;
            let card = &self.state.players[player].battlefield[index];
            let owner_index = self.player_index(&card.owner)?;
            Some(self.prepare_graveyard_destination(owner_index, card))
        } else {
            None
        };
        let observers = self.capture_battlefield_departure_trigger_sources();
        self.remove_prepared_spell_copy(instance_id);
        let Some((controller_index, card_index)) = self.permanent_position(instance_id) else {
            return Ok(None);
        };
        let was_earthbent = self.state.players[controller_index].battlefield[card_index]
            .flags
            .get("earthbent")
            .copied()
            .unwrap_or(false);
        let target_numeric_departure =
            self.target_numeric_departure_for_install(instance_id, prepared_target_numeric)?;
        self.capture_pending_source_lki(instance_id);
        let mut card = self.state.players[controller_index]
            .battlefield
            .remove(card_index);
        self.confirm_target_numeric_departure(target_numeric_departure);
        let trigger_source = card.clone();
        reset_movement_card(&mut card, &movement_source)?;
        card.controller = card.owner.clone();
        let owner_index = self.player_index(&card.owner)?;
        let entered_graveyard = match destination {
            FrozenGraveyardDestination::EvaluateLive => {
                let plan = live_destination.ok_or_else(|| {
                    EngineError::new("missing prepared live graveyard destination")
                })?;
                self.put_owned_card_into_prepared_graveyard(
                    owner_index,
                    OwnedMovementCard {
                        source: movement_source,
                        card,
                    },
                    plan,
                )?
            }
            FrozenGraveyardDestination::Graveyard => {
                let actual = ObjectZone::Graveyard(card.owner.clone());
                self.commit_card_zone_transition(
                    &mut card,
                    &movement_source,
                    &actual,
                    ObjectTransitionReason::Move,
                )?;
                self.state.players[owner_index].graveyard.push(card);
                true
            }
            FrozenGraveyardDestination::Exile {
                replacement_source_id,
                replacement_controller_id,
            } => {
                let owner_id = self.state.players[owner_index].id.clone();
                self.commit_card_zone_transition(
                    &mut card,
                    &movement_source,
                    &ObjectZone::Exile,
                    ObjectTransitionReason::Move,
                )?;
                self.state.players[owner_index].exile.push(card);
                let mut details = json!({
                    "destination": "exile",
                    "replacementSourceId": replacement_source_id,
                });
                if let Some(replacement_controller_id) = replacement_controller_id {
                    details["replacementControllerId"] = Value::String(replacement_controller_id);
                }
                self.record_event(
                    "graveyardDestinationReplaced",
                    Some(owner_id),
                    Some(instance_id.to_string()),
                    details,
                );
                false
            }
        };
        let destination = if entered_graveyard {
            "graveyard"
        } else {
            "exile"
        };
        Ok(Some(InstalledBattlefieldDeparture {
            permanent: trigger_source,
            destination,
            was_earthbent,
            event: DepartureEvent::GraveyardMove,
            observers,
        }))
    }
    pub(super) fn install_permanent_exile_move(
        &mut self,
        instance_id: &str,
    ) -> Result<Option<InstalledBattlefieldDeparture>, EngineError> {
        self.install_permanent_exile_move_with_target_numeric(instance_id, None)
    }

    pub(super) fn install_permanent_exile_move_with_target_numeric(
        &mut self,
        instance_id: &str,
        prepared_target_numeric: Option<super::target_numeric_lki::PreparedTargetDeparture>,
    ) -> Result<Option<InstalledBattlefieldDeparture>, EngineError> {
        if self.permanent_position(instance_id).is_none() {
            return Ok(None);
        }
        let movement_source = self
            .prepare_card_movement_source(instance_id, None)?
            .ok_or_else(|| EngineError::new("battlefield exile source disappeared"))?;
        let observers = self.capture_battlefield_departure_trigger_sources();
        self.state.rule_modifiers.retain(|modifier| {
            !(matches!(
                value_kind(modifier),
                Some("exileIfLeavesBattlefield" | "exileAtEndStep")
            ) && modifier["cardInstanceId"].as_str() == Some(instance_id))
        });
        self.remove_prepared_spell_copy(instance_id);
        let Some((controller_index, card_index)) = self.permanent_position(instance_id) else {
            return Ok(None);
        };
        let was_earthbent = self.state.players[controller_index].battlefield[card_index]
            .flags
            .get("earthbent")
            .copied()
            .unwrap_or(false);
        let target_numeric_departure =
            self.target_numeric_departure_for_install(instance_id, prepared_target_numeric)?;
        self.capture_pending_source_lki(instance_id);
        let mut card = self.state.players[controller_index]
            .battlefield
            .remove(card_index);
        self.confirm_target_numeric_departure(target_numeric_departure);
        let last_known = card.clone();
        reset_movement_card(&mut card, &movement_source)?;
        card.controller = card.owner.clone();
        let owner_index = self.player_index(&card.owner)?;
        self.commit_card_zone_transition(
            &mut card,
            &movement_source,
            &ObjectZone::Exile,
            ObjectTransitionReason::Move,
        )?;
        self.state.players[owner_index].exile.push(card);
        Ok(Some(InstalledBattlefieldDeparture {
            permanent: last_known,
            destination: "exile",
            was_earthbent,
            event: DepartureEvent::Exile,
            observers,
        }))
    }
    pub(super) fn publish_installed_battlefield_departure(
        &mut self,
        movement: &InstalledBattlefieldDeparture,
        reason: &str,
    ) -> Result<(), EngineError> {
        self.publish_installed_battlefield_departure_with_sources(movement, reason, None)
    }

    pub(super) fn publish_installed_battlefield_departure_with_sources(
        &mut self,
        movement: &InstalledBattlefieldDeparture,
        reason: &str,
        captured: Option<&[DepartureTriggerSource]>,
    ) -> Result<(), EngineError> {
        self.publish_installed_battlefield_departure_callbacks(movement, reason, captured, None)?;
        self.finish_battlefield_departure_continuation(movement)
    }

    pub(super) fn finish_battlefield_departure_continuation(
        &mut self,
        movement: &InstalledBattlefieldDeparture,
    ) -> Result<(), EngineError> {
        self.check_quantity_failure()?;
        if movement.was_earthbent {
            self.return_earthbent_card(&movement.permanent.instance_id, movement.destination)?;
        }
        Ok(())
    }

    pub(super) fn record_installed_battlefield_departure(
        &mut self,
        movement: &InstalledBattlefieldDeparture,
        reason: &str,
    ) {
        let source = &movement.permanent;
        let instance_id = &source.instance_id;
        // Last-known characteristics (CR 603.10a, 608.2h): the effects
        // that applied to the permanent as it last existed decide whether it
        // was a creature (an animated land that dies).
        let was_creature = self.permanent_is_creature(source);
        match movement.event {
            DepartureEvent::GraveyardMove => {
                self.record_event(
                    if movement.destination == "graveyard" {
                        "permanentToGraveyard"
                    } else {
                        "permanentToExile"
                    },
                    None,
                    Some(instance_id.clone()),
                    json!({ "reason": reason }),
                );
                if movement.destination == "graveyard" {
                    self.record_event("permanentDied", Some(source.controller.clone()), Some(instance_id.clone()),
                        json!({ "reason": reason, "typeLine": source.definition.type_line.clone(), "wasCreature": was_creature, "owner": source.owner.clone() }));
                }
            }
            DepartureEvent::Exile => {
                self.record_event(
                    "permanentExiled",
                    Some(source.controller.clone()),
                    Some(instance_id.clone()),
                    json!({ "typeLine": source.definition.type_line.clone() }),
                );
            }
        }
        self.record_event("permanentLeftBattlefield", Some(source.controller.clone()), Some(instance_id.clone()),
            json!({ "destination": movement.destination, "typeLine": source.definition.type_line.clone(), "wasCreature": was_creature, "owner": source.owner.clone() }));
    }

    pub(super) fn publish_installed_battlefield_leave_triggers(
        &mut self,
        movement: &InstalledBattlefieldDeparture,
        captured_sources: Option<&[DepartureTriggerSource]>,
        captured_power: Option<i32>,
    ) {
        let source = &movement.permanent;
        self.enqueue_permanent_left_triggers_with_sources(
            source,
            movement.destination,
            captured_sources.unwrap_or(&movement.observers),
            captured_power,
        );
    }

    pub(super) fn cleanup_installed_battlefield_departure(
        &mut self,
        movement: &InstalledBattlefieldDeparture,
    ) {
        if self.check_quantity_failure().is_err() {
            return;
        }
        let instance_id = &movement.permanent.instance_id;
        self.handle_attachment_target_left(instance_id);
        if self.check_quantity_failure().is_err() {
            return;
        }
        self.refresh_control_effects();
    }

    pub(super) fn publish_installed_battlefield_departure_callbacks(
        &mut self,
        movement: &InstalledBattlefieldDeparture,
        reason: &str,
        captured_sources: Option<&[DepartureTriggerSource]>,
        captured_power: Option<i32>,
    ) -> Result<(), EngineError> {
        self.record_installed_battlefield_departure(movement, reason);
        if movement.destination == "graveyard" {
            self.enqueue_cards_entered_graveyard_triggers_from(
                &movement.permanent.owner,
                std::slice::from_ref(&movement.permanent),
                "battlefield",
            );
            self.check_quantity_failure()?;
        }
        self.publish_installed_battlefield_leave_triggers(
            movement,
            captured_sources,
            captured_power,
        );
        self.check_quantity_failure()?;
        self.cleanup_installed_battlefield_departure(movement);
        self.check_quantity_failure()?;
        Ok(())
    }
    pub(super) fn commit_permanent_graveyard_move(
        &mut self,
        instance_id: &str,
        reason: &str,
        destination: FrozenGraveyardDestination,
    ) -> Result<Option<CardInstance>, EngineError> {
        self.commit_permanent_graveyard_move_with_sources(instance_id, reason, destination, None)
    }

    pub(super) fn commit_permanent_graveyard_move_with_sources(
        &mut self,
        instance_id: &str,
        reason: &str,
        destination: FrozenGraveyardDestination,
        captured: Option<&[DepartureTriggerSource]>,
    ) -> Result<Option<CardInstance>, EngineError> {
        let Some(movement) = self.install_permanent_graveyard_move(instance_id, destination)?
        else {
            return Ok(None);
        };
        let died = movement.died();
        self.publish_installed_battlefield_departure_with_sources(&movement, reason, captured)?;
        Ok(died)
    }
    pub(super) fn install_planned_graveyard_departure(
        &mut self,
        permanent: &CardInstance,
        plan: &PlannedGraveyardDeparture,
        reason: &str,
    ) -> Result<InstalledGraveyardDeparture, EngineError> {
        let instance_id = &permanent.instance_id;
        if plan.consume_specific_graveyard_replacement {
            self.state.rule_modifiers.retain(|modifier| {
                !(value_kind(modifier) == Some("exileSpecificCardInsteadOfGraveyard")
                    && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str()))
            });
        }
        let mut on_replaced = None;
        let mut after_event = None;
        let movement = match &plan.outcome {
            PlannedGraveyardOutcome::DeathReplacedWithExile {
                source,
                on_replaced: ability,
            } => {
                self.record_event(
                    "deathReplacedWithExile",
                    None,
                    Some(instance_id.clone()),
                    json!({ "reason": reason }),
                );
                let movement = self.install_permanent_exile_move_with_target_numeric(
                    instance_id,
                    plan.target_numeric_snapshot.clone(),
                )?;
                if let Some(ability) = ability {
                    on_replaced = Some((source.clone(), ability.clone()));
                }
                movement
            }
            PlannedGraveyardOutcome::LeavingBattlefieldReplacedWithExile => {
                after_event = Some(AfterDepartureEvent::LeavingReplacement);
                self.install_permanent_exile_move_with_target_numeric(
                    instance_id,
                    plan.target_numeric_snapshot.clone(),
                )?
            }
            PlannedGraveyardOutcome::DeathReplacedByRuleModifier { modifier_kind } => {
                let turn_number = self.state.turn_number;
                self.state.rule_modifiers.retain(|modifier| {
                    let applies = match modifier_kind.as_str() {
                        "exileIfWouldDie" => {
                            value_kind(modifier) == Some("exileIfWouldDie")
                                && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str())
                        }
                        "exileIfDiesThisTurn" => {
                            value_kind(modifier) == Some("exileIfDiesThisTurn")
                                && modifier["instanceId"].as_str() == Some(instance_id.as_str())
                                && modifier["expiresAfterTurn"].as_u64()
                                    == Some(u64::from(turn_number))
                        }
                        _ => false,
                    };
                    !applies
                });
                after_event = Some(AfterDepartureEvent::DeathRuleReplacement);
                self.install_permanent_exile_move_with_target_numeric(
                    instance_id,
                    plan.target_numeric_snapshot.clone(),
                )?
            }
            PlannedGraveyardOutcome::MoveTo(destination) => self
                .install_permanent_graveyard_move_with_target_numeric(
                    instance_id,
                    destination.clone(),
                    plan.target_numeric_snapshot.clone(),
                )?,
        };
        Ok(InstalledGraveyardDeparture {
            movement,
            on_replaced,
            after_event,
        })
    }
    pub(super) fn publish_installed_graveyard_departure(
        &mut self,
        installed: &InstalledGraveyardDeparture,
        reason: &str,
    ) -> Result<(), EngineError> {
        self.publish_installed_graveyard_departure_with_sources(installed, reason, None)
    }

    pub(super) fn publish_installed_graveyard_departure_with_sources(
        &mut self,
        installed: &InstalledGraveyardDeparture,
        reason: &str,
        captured: Option<&[DepartureTriggerSource]>,
    ) -> Result<(), EngineError> {
        let Some(movement) = &installed.movement else {
            return Ok(());
        };
        self.publish_installed_battlefield_departure_with_sources(movement, reason, captured)?;
        self.publish_graveyard_departure_replacement_event(installed, reason);
        Ok(())
    }

    pub(super) fn publish_graveyard_departure_replacement_event(
        &mut self,
        installed: &InstalledGraveyardDeparture,
        reason: &str,
    ) {
        let Some(movement) = &installed.movement else {
            return;
        };
        match installed.after_event {
            Some(AfterDepartureEvent::LeavingReplacement) => self.record_event(
                "zoneChangeReplacedByExile",
                None,
                Some(movement.permanent.instance_id.clone()),
                json!({ "intendedDestination": "graveyard" }),
            ),
            Some(AfterDepartureEvent::DeathRuleReplacement) => self.record_event(
                "deathReplacedByExile",
                None,
                Some(movement.permanent.instance_id.clone()),
                json!({ "reason": reason }),
            ),
            None => {}
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "battlefield_departure_tests.rs"]
mod tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "battlefield_zone_writer_tests.rs"]
mod zone_writer_tests;
