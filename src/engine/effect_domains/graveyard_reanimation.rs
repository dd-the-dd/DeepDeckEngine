//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::card_zone_transition::{
    ObjectTransitionReason, ObjectZone, reset_owned_movement_card,
};
use super::super::*;

pub(super) fn node_requires_checkpoint(value: &Value) -> bool {
    matches!(value_kind(value), Some("returnCardsFromGraveyard"))
}

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("returnCardsFromGraveyard") => {
            player_reference_supported(&effect["player"])
                && player_reference_supported(&effect["controller"])
                && card_filter_supported(&effect["where"])
                && effect.get("tapped").is_none_or(Value::is_boolean)
        }
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_graveyard_reanimation_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_graveyard_reanimation_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_graveyard_reanimation_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("returnCardsFromGraveyard") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let controller_id = self
                    .resolve_player(&effect["controller"], stack_object, bindings)
                    .unwrap_or_else(|| player_id.clone());
                let player_index = self.player_index(&player_id)?;
                let entry_policy = self.capture_zone_entry_policy();
                let selected_ids = self.state.players[player_index]
                    .graveyard
                    .iter()
                    .filter(|card| {
                        matches_card_filter_with_decisions(
                            &card.definition,
                            &effect["where"],
                            decisions,
                        )
                    })
                    .map(|card| card.instance_id.clone())
                    .collect::<Vec<_>>();
                let mut moved = Vec::new();
                for instance_id in &selected_ids {
                    if entry_policy
                        .admission_for_id(self, instance_id, "graveyard")
                        .is_prohibited()
                    {
                        continue;
                    }
                    let constraint = ("graveyard".to_string(), player_index);
                    if let Some(source) =
                        self.prepare_card_movement_source(instance_id, Some(&constraint))?
                    {
                        moved.push(self.take_prepared_movement_card(source)?);
                    }
                }
                let controller_index = self.player_index(&controller_id)?;
                let moved_ids = moved
                    .iter()
                    .map(|card| card.instance_id.clone())
                    .collect::<Vec<_>>();
                for mut card in moved {
                    reset_owned_movement_card(&mut card)?;
                    if entry_policy.destination_replacement(&card.card.definition) == Some("exile")
                    {
                        card.card.controller = card.card.owner.clone();
                        let owner_index = self.player_index(&card.card.owner)?;
                        self.commit_movement_identity(
                            &mut card,
                            &ObjectZone::Exile,
                            ObjectTransitionReason::Move,
                        )?;
                        self.state.players[owner_index].exile.push(card.card);
                        continue;
                    }
                    card.flags.insert("enteredFrom:graveyard".to_string(), true);
                    card.controller = controller_id.clone();
                    card.summoning_sick = true;
                    card.tapped = effect["tapped"].as_bool().unwrap_or(false);
                    self.apply_enter_replacements(&mut card, provider)?;
                    self.commit_movement_identity(
                        &mut card,
                        &ObjectZone::Battlefield,
                        ObjectTransitionReason::Move,
                    )?;
                    let trigger_source = card.card.clone();
                    self.state.players[controller_index]
                        .battlefield
                        .push(card.card);
                    self.enqueue_enter_battlefield_triggers(&trigger_source);
                }
                self.enqueue_cards_left_graveyard_triggers(&player_id, &moved_ids);
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
