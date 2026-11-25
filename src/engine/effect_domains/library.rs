//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("drawCards") => {
            numeric_expression_supported(&effect["count"])
                && (player_reference_supported(&effect["player"])
                    || value_kind(&effect["player"]) == Some("chosenTargets")
                        && effect["player"]["id"].is_string())
                && effect.get("bind").is_none_or(Value::is_string)
                && effect.get("bindCountAs").is_none_or(Value::is_string)
        }
        Some("mill") => {
            numeric_expression_supported(&effect["count"])
                && player_reference_supported(&effect["player"])
                && effect.get("bind").is_none_or(Value::is_string)
        }

        Some("recordLibrarySearch") => {
            player_reference_supported(&effect["player"])
                && object_expression_supported(&effect["cards"])
        }

        Some("revealCards") => object_expression_supported(&effect["cards"]),
        Some("shuffleZone") => {
            library_zone_supported(&effect["zone"])
                && effect
                    .get("excludeCards")
                    .is_none_or(object_expression_supported)
        }

        Some("exileFromTopUntil") => exile_from_top_until_supported(effect),
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_library_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_library_domain_known(effect, stack_object, bindings, decisions, provider)
            .map(|()| true)
    }

    fn execute_library_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("exileFromTopUntil") => {
                self.exile_from_top_until(effect, stack_object, bindings)?;
            }
            Some("drawCards") => {
                if self.numeric_amount_requires_initially_illegal_target(
                    &effect["count"],
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    return Ok(());
                }
                let player_ids = match value_kind(&effect["player"]) {
                    Some("chosenTargets") => {
                        let target_id = effect["player"]["id"].as_str().unwrap_or_default();
                        stack_object
                            .targets
                            .iter()
                            .filter(|(id, _)| {
                                id.as_str() == target_id || id.starts_with(&format!("{target_id}:"))
                            })
                            .filter_map(|(_, target)| match target {
                                TargetRef::Player { player_id } => Some(player_id.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    }
                    Some("opponentsOf") => {
                        let reference_player = self.resolve_player(
                            &effect["player"]["player"],
                            stack_object,
                            bindings,
                        );
                        self.state
                            .players
                            .iter()
                            .filter(|player| {
                                !player.has_lost
                                    && reference_player
                                        .as_deref()
                                        .is_some_and(|reference| player.id != reference)
                            })
                            .map(|player| player.id.clone())
                            .collect::<Vec<_>>()
                    }
                    Some("eachPlayer") => self
                        .state
                        .players
                        .iter()
                        .filter(|player| !player.has_lost)
                        .map(|player| player.id.clone())
                        .collect::<Vec<_>>(),
                    _ => self
                        .resolve_player(&effect["player"], stack_object, bindings)
                        .into_iter()
                        .collect::<Vec<_>>(),
                };
                let count = self
                    .checked_runtime_numeric_expression(
                        &effect["count"],
                        stack_object,
                        bindings,
                        decisions,
                        "drawCards.count",
                    )?
                    .max(0) as usize;
                let mut drawn_ids = Vec::new();
                for player_id in player_ids {
                    let first_new_event = self.state.events.len();
                    self.draw_cards_with_replacements(&player_id, count, provider)?;
                    drawn_ids.extend(
                        self.state.events[first_new_event..]
                            .iter()
                            .filter(|event| {
                                event.kind == "cardDrawn"
                                    && event.player_id.as_deref() == Some(player_id.as_str())
                            })
                            .filter_map(|event| event.card_instance_id.clone()),
                    );
                }
                if let Some(binding_id) = effect["bind"].as_str() {
                    bindings.insert(binding_id.to_string(), RuntimeBinding::Objects(drawn_ids));
                }
            }

            Some("mill") => {
                if self.value_requires_initially_illegal_target_in_context(
                    &effect["player"],
                    stack_object,
                    bindings,
                    decisions,
                )? || self.numeric_amount_requires_initially_illegal_target(
                    &effect["count"],
                    stack_object,
                    bindings,
                    decisions,
                )? {
                    return Ok(());
                }
                let player_id = self
                    .resolve_player(&effect["player"], stack_object, bindings)
                    .ok_or_else(|| EngineError::new("unresolved mill player"))?;
                let count = self
                    .checked_runtime_numeric_expression(
                        &effect["count"],
                        stack_object,
                        bindings,
                        decisions,
                        "mill.count",
                    )?
                    .max(0) as usize;
                let milled = self.mill_cards_with(
                    &player_id,
                    count,
                    Some(&stack_object.controller),
                    Some(&mut *provider as &mut dyn DecisionProvider),
                )?;
                if let Some(binding_id) = effect["bind"].as_str() {
                    bindings.insert(binding_id.to_string(), RuntimeBinding::Objects(milled));
                }
            }

            Some("recordLibrarySearch") => {
                let Some(player_id) =
                    self.resolve_player(&effect["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let card_ids = self
                    .resolve_runtime_object_ids(&effect["cards"], stack_object, bindings, decisions)
                    .unwrap_or_default();
                self.record_event(
                    "librarySearched",
                    Some(player_id.clone()),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "cardIds": card_ids }),
                );
                self.enqueue_library_searched_triggers(&player_id);
            }
            Some("revealCards") => {
                let Some(objects) = self.resolve_runtime_object_ids_for_effect(
                    &effect["cards"],
                    stack_object,
                    bindings,
                    decisions,
                )?
                else {
                    return Ok(());
                };
                if !objects.is_empty() {
                    self.record_event(
                        "cardsRevealed",
                        Some(stack_object.controller.clone()),
                        Some(stack_object.card.instance_id.clone()),
                        json!({ "cardIds": objects, "stackId": stack_object.id }),
                    );
                }
            }
            Some("shuffleZone") => {
                let excluded_card_ids = effect
                    .get("excludeCards")
                    .and_then(|objects| self.resolve_object_ids(objects, bindings, decisions))
                    .unwrap_or_default();
                let Some(player_id) =
                    self.resolve_player(&effect["zone"]["player"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let player_index = self.player_index(&player_id)?;
                library_shuffle::shuffle_library_excluding(
                    &mut self.state.players[player_index].library,
                    &excluded_card_ids,
                    &mut self.rng,
                );
                self.record_event(
                    "zoneShuffled",
                    Some(player_id),
                    Some(stack_object.card.instance_id.clone()),
                    json!({ "zone": "library", "stackId": stack_object.id }),
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
