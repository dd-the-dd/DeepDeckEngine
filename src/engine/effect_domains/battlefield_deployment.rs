//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::card_zone_transition::{
    ObjectTransitionReason, ObjectZone, reset_owned_movement_card,
};
use super::super::*;

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    matches!(
        value_kind(value),
        Some("putCardsFromHandOntoBattlefield" | "moveAbilitySourceToBattlefield")
    )
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("putCardsFromHandOntoBattlefield") => {
            player_reference_supported(&effect["player"]) && card_filter_supported(&effect["where"])
        }

        Some("moveAbilitySourceToBattlefield") => {
            matches!(effect["from"].as_str(), Some("hand" | "graveyard"))
                && effect["tapped"].is_boolean()
                // CR 702.49c: entering attacking a recorded defender.
                && effect
                    .get("attacking")
                    .is_none_or(super::super::attacking_entry::attacking_entry_supported)
                && effect.get("enterWithCounters").is_none_or(|counters| {
                    counters.as_array().is_some_and(|counters| {
                        counters.iter().all(|counter| {
                            counter["counter"].is_string()
                                && numeric_expression_supported(&counter["count"])
                        })
                    })
                })
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_battlefield_deployment_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_battlefield_deployment_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_battlefield_deployment_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("moveAbilitySourceToBattlefield") => {
                let player_index = self.player_index(&stack_object.card.owner)?;
                let from = effect["from"].as_str().unwrap_or_default();
                let admission_zone = if from == "hand" { "hand" } else { "graveyard" };
                let entry_policy = self.capture_zone_entry_policy();
                if entry_policy
                    .admission_for_id(self, &stack_object.card.instance_id, admission_zone)
                    .is_prohibited()
                {
                    return Ok(());
                }
                let constraint = (admission_zone.to_string(), player_index);
                if let Some(source) = self.prepare_card_movement_source(
                    &stack_object.card.instance_id,
                    Some(&constraint),
                )? {
                    let mut card = self.take_prepared_movement_card(source)?;
                    if entry_policy.destination_replacement(&card.card.definition) == Some("exile")
                    {
                        reset_owned_movement_card(&mut card)?;
                        card.card.controller = card.card.owner.clone();
                        let owner_index = self.player_index(&card.card.owner)?;
                        self.commit_movement_identity(
                            &mut card,
                            &ObjectZone::Exile,
                            ObjectTransitionReason::Move,
                        )?;
                        self.state.players[owner_index].exile.push(card.card);
                        return Ok(());
                    }
                    card.controller = stack_object.controller.clone();
                    card.tapped = effect["tapped"].as_bool().unwrap_or(false);
                    card.summoning_sick = true;
                    for counter in effect["enterWithCounters"].as_array().into_iter().flatten() {
                        let Some(counter_name) = counter["counter"].as_str() else {
                            continue;
                        };
                        let count = self
                            .runtime_numeric_expression(
                                &counter["count"],
                                stack_object,
                                bindings,
                                decisions,
                            )
                            .unwrap_or(0)
                            .max(0);
                        if count > 0 {
                            *card.counters.entry(counter_name.to_string()).or_default() += count;
                        }
                    }
                    self.apply_enter_replacements(&mut card, provider)?;
                    if effect["tapped"].as_bool().unwrap_or(false) {
                        card.tapped = true;
                    }
                    self.commit_movement_identity(
                        &mut card,
                        &ObjectZone::Battlefield,
                        ObjectTransitionReason::Move,
                    )?;
                    let trigger_source = card.card.clone();
                    self.state.players[player_index].battlefield.push(card.card);
                    if let Some(attacking) = effect.get("attacking") {
                        self.put_entered_permanent_attacking(
                            &trigger_source.instance_id,
                            attacking,
                            stack_object,
                        );
                    }
                    self.record_event(
                        "abilitySourceMovedToBattlefield",
                        Some(stack_object.controller.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "from": from, "tapped": effect["tapped"] }),
                    );
                    self.enqueue_enter_battlefield_triggers(&trigger_source);
                }
            }

            Some("putCardsFromHandOntoBattlefield") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                let candidates = self.state.players[player_index]
                    .hand
                    .iter()
                    .filter(|card| matches_card_filter(&card.definition, &effect["where"]))
                    .map(|card| card.instance_id.clone())
                    .collect::<Vec<_>>();
                let selected = self.choose_resolution_objects(
                    stack_object,
                    provider,
                    &player_id,
                    "handCardsToBattlefield",
                    candidates.clone(),
                    0,
                    candidates.len(),
                    "Choose any number of cards to put onto the battlefield.",
                )?;
                let entry_policy = self.capture_zone_entry_policy();
                let mut entering_cards = Vec::new();
                for instance_id in selected {
                    if entry_policy
                        .admission_for_id(self, &instance_id, "hand")
                        .is_prohibited()
                    {
                        continue;
                    }
                    let player_index = self.player_index(&player_id)?;
                    let constraint = ("hand".to_string(), player_index);
                    let Some(source) =
                        self.prepare_card_movement_source(&instance_id, Some(&constraint))?
                    else {
                        continue;
                    };
                    let replacement = {
                        let card =
                            self.card_outside_battlefield(&instance_id).ok_or_else(|| {
                                EngineError::new("selected hand entry source disappeared")
                            })?;
                        entry_policy.destination_replacement(&card.definition)
                    };
                    let mut card = self.take_prepared_movement_card(source)?;
                    if replacement.as_deref() == Some("exile") {
                        reset_owned_movement_card(&mut card)?;
                        card.controller = card.owner.clone();
                        let owner_index = self.player_index(&card.owner)?;
                        self.commit_movement_identity(
                            &mut card,
                            &ObjectZone::Exile,
                            ObjectTransitionReason::Move,
                        )?;
                        self.state.players[owner_index].exile.push(card.card);
                        continue;
                    }
                    reset_owned_movement_card(&mut card)?;
                    card.flags.insert("enteredFrom:hand".to_string(), true);
                    card.controller = player_id.clone();
                    card.summoning_sick = true;
                    self.apply_enter_replacements(&mut card, provider)?;
                    self.commit_movement_identity(
                        &mut card,
                        &ObjectZone::Battlefield,
                        ObjectTransitionReason::Move,
                    )?;
                    entering_cards.push(card.card);
                }
                let trigger_sources = entering_cards.clone();
                let player_index = self.player_index(&player_id)?;
                self.state.players[player_index]
                    .battlefield
                    .extend(entering_cards);
                for source in trigger_sources {
                    self.record_event(
                        "cardMovedToBattlefield",
                        Some(player_id.clone()),
                        Some(source.instance_id.clone()),
                        json!({ "from": "hand", "tapped": source.tapped }),
                    );
                    self.ensure_prepared_spell_copy(&source.instance_id)?;
                    self.enqueue_enter_battlefield_triggers(&source);
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
