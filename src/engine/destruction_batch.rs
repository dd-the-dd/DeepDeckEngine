//! Stable planning for a collection of permanents destroyed by one instruction.
//!
//! A collection is selected before any member is destroyed. Protections,
//! replacement effects, and trigger sources that apply to that destruction must
//! therefore be captured from the same pre-destruction battlefield. Re-reading
//! the battlefield after every removal makes the outcome depend on iteration
//! order.

use super::*;

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
enum PlannedDestructionOutcome {
    UmbraArmor {
        aura_id: String,
    },
    DeathReplacedWithExile {
        source: CardInstance,
        on_replaced: Option<Value>,
    },
    Regenerated,
    Indestructible,
    ShieldCounter,
    LeavingBattlefieldReplacedWithExile,
    DeathReplacedByRuleModifier {
        modifier_kind: String,
    },
    MoveTo(FrozenGraveyardDestination),
}

impl PlannedDestructionOutcome {
    fn counts_as_destroyed(&self) -> bool {
        matches!(
            self,
            Self::DeathReplacedWithExile { .. }
                | Self::LeavingBattlefieldReplacedWithExile
                | Self::DeathReplacedByRuleModifier { .. }
                | Self::MoveTo(_)
        )
    }
}

struct PlannedDestruction {
    permanent: CardInstance,
    remove_regeneration_shield: bool,
    consume_specific_graveyard_replacement: bool,
    outcome: PlannedDestructionOutcome,
}

struct DestructionTriggerSource {
    source: CardInstance,
    rules: Vec<Value>,
}

struct DestructionTriggerSourceSnapshot {
    sources: Vec<DestructionTriggerSource>,
}

struct BatchPublication {
    permanent: CardInstance,
    died: Option<CardInstance>,
    destroyed: bool,
    on_replaced: Option<(CardInstance, Value)>,
}

fn exactly_one_or_none<T>(
    mut values: Vec<T>,
    ambiguity: impl FnOnce() -> EngineError,
) -> Result<Option<T>, EngineError> {
    if values.len() > 1 {
        return Err(ambiguity());
    }
    Ok(values.pop())
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

fn validate_zone_or_death_replacement_choice(
    instance_id: &str,
    has_death_replacement: bool,
    has_leaving_replacement: bool,
    has_one_shot_death_replacement: bool,
    has_graveyard_replacement: bool,
) -> Result<(), EngineError> {
    let applicable = usize::from(has_death_replacement)
        + usize::from(has_leaving_replacement)
        + usize::from(has_one_shot_death_replacement)
        + usize::from(has_graveyard_replacement);
    if applicable > 1 {
        return Err(EngineError::new(format!(
            "destruction batch has multiple applicable zone or death replacements for {instance_id}"
        )));
    }
    Ok(())
}

impl GameEngine {
    fn capture_destruction_trigger_sources(&self) -> DestructionTriggerSourceSnapshot {
        let sources = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .filter_map(|source| {
                let rules = source
                    .definition
                    .rules
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_active_for_source(source, rule)
                            && value_kind(&rule["event"])
                                == Some("opponentDestroyedControlledPermanent")
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                (!rules.is_empty()).then(|| DestructionTriggerSource {
                    source: source.clone(),
                    rules,
                })
            })
            .collect();
        DestructionTriggerSourceSnapshot { sources }
    }

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
                    consume_specific_graveyard_replacement: false,
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
                    consume_specific_graveyard_replacement: false,
                    outcome: PlannedDestructionOutcome::UmbraArmor { aura_id },
                });
                continue;
            }

            if has_regeneration_shield {
                planned.push(PlannedDestruction {
                    permanent,
                    remove_regeneration_shield: false,
                    consume_specific_graveyard_replacement: false,
                    outcome: PlannedDestructionOutcome::Regenerated,
                });
                continue;
            }

            if has_shield_counter {
                planned.push(PlannedDestruction {
                    permanent,
                    remove_regeneration_shield: false,
                    consume_specific_graveyard_replacement: false,
                    outcome: PlannedDestructionOutcome::ShieldCounter,
                });
                continue;
            }

            let death_replacement = exactly_one_or_none(
                static_modifiers
                    .iter()
                    .filter(|(source, modifier)| {
                        value_kind(modifier) == Some("exileOpponentCreatureInsteadOfDying")
                            && source.controller != permanent.controller
                            && modifier.get("where").map_or_else(
                                || self.permanent_is_creature(&permanent),
                                |where_filter| {
                                    self.permanent_matches_effective_filter(
                                        &permanent,
                                        where_filter,
                                    )
                                },
                            )
                    })
                    .map(|(source, modifier)| (source.clone(), modifier.get("onReplaced").cloned()))
                    .collect::<Vec<_>>(),
                || {
                    EngineError::new(format!(
                        "destruction batch has multiple death replacements for {instance_id}"
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
                            if modifier["cardInstanceId"].as_str()
                                == Some(instance_id.as_str()) =>
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
                        "destruction batch has multiple one-shot death replacements for {instance_id}"
                    ))
                },
            )?;
            let owner_id = permanent.owner.clone();
            let specific_replacement = self
                .state
                .rule_modifiers
                .iter()
                .filter(|modifier| {
                    value_kind(modifier) == Some("exileSpecificCardInsteadOfGraveyard")
                        && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str())
                })
                .map(|modifier| {
                    (
                        modifier["sourceCardInstanceId"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        None,
                        true,
                    )
                });
            let static_replacements = static_modifiers
                .iter()
                .filter(|(source, modifier)| {
                    value_kind(modifier) == Some("replaceGraveyardDestination")
                        && matches_card_filter(&permanent.definition, &modifier["where"])
                        && self.event_player_reference_matches(
                            &modifier["graveyardOwners"],
                            source,
                            &owner_id,
                        )
                })
                .map(|(source, _)| {
                    (
                        source.instance_id.clone(),
                        Some(source.controller.clone()),
                        false,
                    )
                });
            let temporary_replacements = self
                .state
                .rule_modifiers
                .iter()
                .filter(|modifier| {
                    value_kind(modifier) == Some("temporaryGraveyardDestinationReplacement")
                        && modifier["playerId"].as_str() == Some(owner_id.as_str())
                        && modifier["expiresAfterTurn"].as_u64()
                            == Some(u64::from(self.state.turn_number))
                })
                .map(|modifier| {
                    (
                        modifier["sourceCardInstanceId"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                        modifier["playerId"].as_str().map(ToOwned::to_owned),
                        false,
                    )
                });
            let graveyard_replacement = exactly_one_or_none(
                specific_replacement
                    .chain(static_replacements)
                    .chain(temporary_replacements)
                    .collect::<Vec<_>>(),
                || {
                    EngineError::new(format!(
                        "destruction batch has multiple graveyard destination replacements for {instance_id}"
                    ))
                },
            )?;
            validate_zone_or_death_replacement_choice(
                &instance_id,
                death_replacement.is_some(),
                leaving_replacement,
                death_rule_modifier.is_some(),
                graveyard_replacement.is_some(),
            )?;
            let (outcome, consume_specific_graveyard_replacement) =
                if let Some((source, on_replaced)) = death_replacement {
                    (
                        PlannedDestructionOutcome::DeathReplacedWithExile {
                            source,
                            on_replaced,
                        },
                        false,
                    )
                } else if leaving_replacement {
                    (
                        PlannedDestructionOutcome::LeavingBattlefieldReplacedWithExile,
                        false,
                    )
                } else if let Some(modifier_kind) = death_rule_modifier {
                    (
                        PlannedDestructionOutcome::DeathReplacedByRuleModifier { modifier_kind },
                        false,
                    )
                } else if let Some((source_id, source_controller, consume_specific)) =
                    graveyard_replacement
                {
                    (
                        PlannedDestructionOutcome::MoveTo(FrozenGraveyardDestination::Exile {
                            replacement_source_id: source_id,
                            replacement_controller_id: source_controller,
                        }),
                        consume_specific,
                    )
                } else {
                    (
                        PlannedDestructionOutcome::MoveTo(FrozenGraveyardDestination::Graveyard),
                        false,
                    )
                };
            planned.push(PlannedDestruction {
                permanent,
                remove_regeneration_shield: cannot_regenerate,
                consume_specific_graveyard_replacement,
                outcome,
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
        let destruction_trigger_sources = self.capture_destruction_trigger_sources();
        let mut destroyed_ids = Vec::new();
        let mut publications = Vec::new();

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
            if planned_destruction.consume_specific_graveyard_replacement {
                self.state.rule_modifiers.retain(|modifier| {
                    !(value_kind(modifier) == Some("exileSpecificCardInsteadOfGraveyard")
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
                PlannedDestructionOutcome::DeathReplacedWithExile {
                    source,
                    on_replaced,
                } => {
                    self.record_event(
                        "deathReplacedWithExile",
                        None,
                        Some(instance_id.clone()),
                        json!({ "reason": "destroyed" }),
                    );
                    self.exile_permanent(&instance_id)?;
                    if let Some(on_replaced) = on_replaced {
                        on_replaced_publication = Some((source.clone(), on_replaced.clone()));
                    }
                }
                PlannedDestructionOutcome::Regenerated => {
                    if let Some(regeneration_index) =
                        self.state.rule_modifiers.iter().position(|modifier| {
                            value_kind(modifier) == Some("regenerationShield")
                                && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str())
                        })
                    {
                        self.state.rule_modifiers.remove(regeneration_index);
                    }
                    if let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                    {
                        let permanent =
                            &mut self.state.players[player_index].battlefield[card_index];
                        permanent.tapped = true;
                        permanent.damage_marked = 0;
                    }
                    self.record_event(
                        "permanentRegenerated",
                        None,
                        Some(instance_id.clone()),
                        json!({ "reason": "destroyed" }),
                    );
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
                PlannedDestructionOutcome::LeavingBattlefieldReplacedWithExile => {
                    self.exile_permanent(&instance_id)?;
                    self.record_event(
                        "zoneChangeReplacedByExile",
                        None,
                        Some(instance_id.clone()),
                        json!({ "intendedDestination": "graveyard" }),
                    );
                }
                PlannedDestructionOutcome::DeathReplacedByRuleModifier { modifier_kind } => {
                    let turn_number = self.state.turn_number;
                    self.state.rule_modifiers.retain(|modifier| {
                        let applies = match modifier_kind.as_str() {
                            "exileIfWouldDie" => {
                                value_kind(modifier) == Some("exileIfWouldDie")
                                    && modifier["cardInstanceId"].as_str()
                                        == Some(instance_id.as_str())
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
                    self.exile_permanent(&instance_id)?;
                    self.record_event(
                        "deathReplacedByExile",
                        None,
                        Some(instance_id.clone()),
                        json!({ "reason": "destroyed" }),
                    );
                }
                PlannedDestructionOutcome::MoveTo(destination) => {
                    died = self.commit_permanent_graveyard_move(
                        &instance_id,
                        "destroyed",
                        destination.clone(),
                    )?;
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
                self.put_triggered_ability_on_stack(&source, on_replaced);
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
            for trigger_source in &destruction_trigger_sources.sources {
                if trigger_source.source.controller != publication.permanent.controller {
                    continue;
                }
                for rule in &trigger_source.rules {
                    if !matches_card_filter(
                        &publication.permanent.definition,
                        &rule["event"]["where"],
                    ) {
                        continue;
                    }
                    self.put_triggered_ability_on_stack_with_decisions(
                        &trigger_source.source,
                        rule.clone(),
                        BTreeMap::from([(
                            "triggeringPlayerId".to_string(),
                            Value::String(stack_object.controller.clone()),
                        )]),
                    );
                }
            }
        }

        Ok(destroyed_ids)
    }

    pub(super) fn commit_permanent_graveyard_move(
        &mut self,
        instance_id: &str,
        reason: &str,
        destination: FrozenGraveyardDestination,
    ) -> Result<Option<CardInstance>, EngineError> {
        self.remove_prepared_spell_copy(instance_id);
        let Some((controller_index, card_index)) = self.permanent_position(instance_id) else {
            return Ok(None);
        };
        let was_earthbent = self.state.players[controller_index].battlefield[card_index]
            .flags
            .get("earthbent")
            .copied()
            .unwrap_or(false);
        self.capture_pending_source_lki(instance_id);
        let mut card = self.state.players[controller_index]
            .battlefield
            .remove(card_index);
        let trigger_source = card.clone();
        reset_after_leaving_battlefield(&mut card);
        let owner_index = self.player_index(&card.owner)?;
        let entered_graveyard = match destination {
            FrozenGraveyardDestination::EvaluateLive => {
                self.put_card_into_graveyard(owner_index, card)
            }
            FrozenGraveyardDestination::Graveyard => {
                self.state.players[owner_index].graveyard.push(card);
                true
            }
            FrozenGraveyardDestination::Exile {
                replacement_source_id,
                replacement_controller_id,
            } => {
                let owner_id = self.state.players[owner_index].id.clone();
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
        self.record_event(
            if entered_graveyard {
                "permanentToGraveyard"
            } else {
                "permanentToExile"
            },
            None,
            Some(instance_id.to_string()),
            json!({ "reason": reason }),
        );
        if entered_graveyard {
            self.record_event(
                "permanentDied",
                Some(trigger_source.controller.clone()),
                Some(instance_id.to_string()),
                json!({
                    "reason": reason,
                    "typeLine": trigger_source.definition.type_line.clone(),
                    "wasCreature": is_creature(&trigger_source.definition),
                }),
            );
        }
        self.record_event(
            "permanentLeftBattlefield",
            Some(trigger_source.controller.clone()),
            Some(instance_id.to_string()),
            json!({
                "destination": destination,
                "typeLine": trigger_source.definition.type_line,
            }),
        );
        if entered_graveyard {
            self.enqueue_cards_entered_graveyard_triggers(
                &trigger_source.owner,
                std::slice::from_ref(&trigger_source),
            );
        }
        self.enqueue_permanent_left_triggers(&trigger_source, destination);
        self.handle_attachment_target_left(instance_id);
        self.refresh_control_effects();
        if was_earthbent {
            self.return_earthbent_card(instance_id, destination)?;
        }
        Ok(entered_graveyard.then_some(trigger_source))
    }

    pub(super) fn publish_committed_death_triggers(
        &mut self,
        died: &CardInstance,
        reason: &str,
        trigger_sources: &death_trigger_batch::DeathTriggerSourceSnapshot,
    ) {
        self.enqueue_permanent_died_triggers(died, reason, trigger_sources);
        self.enqueue_delayed_death_triggers(died);
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
            self.put_triggered_ability_on_stack(
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
            );
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "destruction_batch_tests.rs"]
mod tests;
